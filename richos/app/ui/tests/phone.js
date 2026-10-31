// USE RICH FROM YOUR PHONE — the pairing screen, its QR code measured FROM THE PIXELS, and the
// one path CEO §61 leaves.
//
// **§61, 2026-09-18:** *"it's an app that lets the user use RichOS (in some way) while being on
// the go and away from office i.e. outside the home network. Because any mobile app or PWA is
// utterly useless within the home network … The Tailscale setup is where we start now."* The
// route chooser, the self-signed certificate, the trust code and the sixteen taps through Apple's
// settings are gone from the product, and the checks that pinned them are gone with them — checks
// 4, 5 and 6 are now about what replaced them, and 27, 29 and 30 about the screens that remain.
// Check 5 is the one that would notice a second path coming back on this surface;
// `tests/no-home-network.js` is the one that would notice it coming back anywhere.
//
// TWO JOBS, AND THE FIRST ONE IS A DEBT THIS FILE EXISTS TO PAY.
//
// `contrast.js` check 14 refuses a `<canvas>` on any walked surface, because a computed-style walk
// cannot read a pixel one painted — *"a green run here must not be read as covering it. If it
// belongs to a surface that IS measured from the pixels somewhere, exclude it here BY NAME and say
// where — never by raising a threshold."* The pairing screen has a canvas, so the exclusion names
// this file, and check 3 below is what makes that name worth something: it reads the rendered
// QR back with `getImageData` in the same WebKit and asserts what is actually painted.
//
// THE SECOND JOB IS THE PORT. `ui/qr.js` is `tools/phone-probe/lib/qr.js` with three changes and
// no more — the source of record is the probe's, which is checked against ISO/IEC 18004's own
// Annex I example and, end to end, by decoding a rendered PNG with Apple's Vision framework.
// Check 1 runs both modules over the same strings and asserts the matrices are identical, module
// for module. That is the only thing a second copy of anything can usefully promise, and without
// it the copy would be a second implementation to be wrong in a second way.
//
// Run: node phone.js   (or `npm test` for every suite in this directory)

"use strict";

const fs = require("fs");
const path = require("path");
const {
  awaitSettled,
  bootSettled,
  leaveHome,
  loadPlaywright,
  shot,
  publishShotFile,
  createRun,
  assert,
  assertEqual,
  UI_DIR,
} = require("./lib/harness");

const APP = "file://" + path.join(UI_DIR, "index.html");
const SHOTS = path.join(__dirname, "shots-phone");

/// The two modules whose agreement check 1 is about. `ui/qr.js` ships; the probe's does not.
const SHIPPED_QR = require(path.join(UI_DIR, "qr.js"));
const PROBE_QR = require(path.resolve(UI_DIR, "..", "..", "tools", "phone-probe", "lib", "qr.js"));

/// The strings the pairing screen actually encodes, plus the two edges of the version table. A
/// cross-check over one happy string would not notice a version boundary handled differently.
///
/// The trust address that used to head this list — `http://mm1.local:8444/ca` — went with the
/// path it belonged to (CEO §61). Its length is kept as an anonymous string, because what this
/// list is for is version boundaries and dropping a length would drop a boundary.
const ENCODED = [
  "https://mm1.tail9a3b2.ts.net:8443/#pair=K7QF2M9X",
  "https://mm1.tail1a2b3c.ts.net:8443/#pair=ZZZZZZZZ",
  "xxxxxxxxxxxxxxxxxxxxxxxx",
  "x",
];

/// **THE MAC EVERY DEFAULT-PRESET CHECK MODELS**, and it is the only Mac the shipped flow opens a
/// pairing window on: signed in, certified, with a name. Before §61 the default was a Mac with no
/// Tailscale at all, which the old second path served happily and this one refuses to — a fixture
/// that cannot reach the screen under test is a fixture that passes for the wrong reason.
const READY_TAILNET = {
  state: "ready",
  name: "mm1.tail9a3b2.ts.net",
  origin: "https://mm1.tail9a3b2.ts.net:8443",
  account: "Google as someone@gmail.com",
};

/// **THE COUNTDOWN, PINNED — FOR THE TWO COMMITTED SHOTS AND NOTHING ELSE.**
///
/// The pairing window is 300 s and the harness's clock starts when `mock.js` loads
/// (`mockPhone.openedAt = now()`), so what the sheet says depends on how long this file took
/// to get from `page.goto` to the paint: `Math.ceil((300000 - elapsed) / 1000)` is 300 for the
/// first second of the page's life and 299 after it, and `remaining()` floors that to `5 more
/// minutes` and `4 more minutes` — a different sentence on either side of ONE millisecond
/// boundary that the walk lands within a hair of.
///
/// MEASURED, 2026-09-20. A probe that opened this sheet twelve times reached the countdown at
/// 879–1,055 ms after `goto` — straddling the boundary — and with the five owning suites of
/// the seven undeclared unstable shots run concurrently (the contention the gate's shards
/// have), `phone-light.png` and `phone-dark.png` came back different in three rounds out of
/// three: 775 of 1,330,000 pixels, worst channel delta 164 and 128, inside one 120x13 box at
/// x[751..870] y[210..222] — `This code lasts 5 more minutes.` against `…4 more minutes.`
///
/// 270 s is `4 more minutes` with THIRTY SECONDS of margin to either edge of its minute, so a
/// shutter anywhere in the first half-minute of the sheet's life photographs one sentence.
/// `settings-fit.js` pins the same knob at 245 for the same reason and gets the same sentence;
/// this one is further from an edge because these pages are photographed rather than read, and
/// the product's own one-second ticker walks `left` down between the sheet's two-second polls.
///
/// IT PINS THE ANSWER, IT DOES NOT MOVE THE CLOCK (`mock.js`'s `phonePairingSecondsLeft`), and
/// it is passed ONLY to the pages the two shots are taken on. Every other check in this file
/// keeps the running clock it was written against — check 7 reads a live countdown and check
/// 25 walks G13's four values, and neither is a question about a photograph.
const SHOT_SECONDS_LEFT = 270;

async function openSheet(browser, theme, preset) {
  const page = await browser.newPage({ viewport: { width: 1400, height: 950 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push("console: " + m.text());
  });
  // THE PRESET HAS TO BE IN PLACE BEFORE ANY OF THE PAGE'S OWN SCRIPTS RUN, because `mock.js`
  // reads it while building its state — `contrast.js` says the same thing from its side.
  await page.addInitScript((v) => {
    window.__RICHOS_MOCK_PRESET__ = v;
  }, preset || { phonePairing: true, phoneTailnet: READY_TAILNET });
  // BOTH THE MIRROR AND THE STORE, for the reason `contrast.js` spells out at length: the
  // mirror is a cache, `syncAppearanceFromBackend` reconciles it against the backend at init,
  // and THE BACKEND WINS. Seeding the mirror alone measures the store's theme while claiming
  // the seed's.
  await page.addInitScript((t) => {
    try {
      window.localStorage.setItem("richos-theme", t);
      window.localStorage.setItem("richos-font-scale", "100");
      window.localStorage.setItem(
        "richos-mock-config",
        JSON.stringify({ theme: t, font_scale: 100, user_name: null })
      );
    } catch (e) {
      /* storage unavailable: theme-boot falls back to the shipped default, which is dark */
    }
  }, theme || "dark");
  await page.goto(APP);
  await leaveHome(page);
  await page.waitForSelector(".nav-thread", { state: "attached" });
  await bootSettled(page);
  await page.click("#set-btn");
  await page.waitForSelector("#set-phone-open");
  await page.click("#set-phone-open");
  await page.waitForSelector("#phone-sheet:not([hidden])");
  page.__errors = errors;
  return page;
}

(async () => {
  const run = createRun("phone", "the pairing screen, its QR code, and the one path §61 leaves");
  const playwright = await loadPlaywright();
  const browser = await playwright.webkit.launch();

  // ---- 1. the port is the probe's encoder, not a second one -------------------------------

  await run.check("1  the shipped QR encoder and the probe's produce identical matrices", async () => {
    for (const text of ENCODED) {
      const mine = SHIPPED_QR.encode(text);
      const theirs = PROBE_QR.encode(text);
      assertEqual(
        { version: mine.version, size: mine.size, mask: mine.mask, modules: mine.modules },
        { version: theirs.version, size: theirs.size, mask: theirs.mask, modules: theirs.modules },
        "app/ui/qr.js and tools/phone-probe/lib/qr.js disagree on " + JSON.stringify(text) +
          " — the shipped copy has drifted from the implementation that is checked against the " +
          "standard and against Apple's own decoder, and the shipped one is the one his phone reads"
      );
    }
    return (
      ENCODED.length + " string(s) encoded identically by both modules, including the two the " +
      "pairing screen actually draws. The probe's is the source of record: it is checked against " +
      "ISO/IEC 18004 Annex I and against Apple's Vision decoder, and neither of those checks is " +
      "re-implemented here."
    );
  });

  await run.check("1b  a URL too long to encode throws rather than drawing something unscannable", async () => {
    let threw = null;
    try {
      SHIPPED_QR.encode("https://" + "x".repeat(200) + ".local:8443/");
    } catch (error) {
      threw = String(error.message || error);
    }
    assert(threw !== null, "a 200-byte URL was encoded — the version table stops at 106 bytes");
    assert(
      /106/.test(threw),
      "the refusal does not name the ceiling it hit, so whoever meets it cannot act on it: " + threw
    );
    return "a URL past the version-6 ceiling throws and names the ceiling: " + threw.slice(0, 90) + "…";
  });

  // ---- 2. both codes are drawn ------------------------------------------------------------

  await run.check("2  the pairing screen draws its code, at the size its content needs", async () => {
    // ONE CODE, NOT TWO. The first of the pair pointed at `http://<name>.local:8444/ca`, the page
    // that served a profile for the phone to install, and CEO §61 removed the path that needed
    // it. The canvas and the address beside it are gone from the markup — check 4 asserts that
    // from the other side.
    const page = await openSheet(browser, "dark");
    const drawn = await page.evaluate(() => {
      const two = document.getElementById("phone-qr-pair");
      return {
        trustCanvasStillThere: !!document.getElementById("phone-qr-trust"),
        pair: { hidden: two.hidden, width: two.width, height: two.height },
        pairUrl: document.getElementById("phone-pair-url").textContent,
      };
    });
    await page.close();
    assert(!drawn.trustCanvasStillThere, "the removed path's QR canvas is still in the markup");
    assert(!drawn.pair.hidden, "the code is hidden: " + JSON.stringify(drawn));
    // Derived rather than remembered: the encoder decides the version from the byte length and
    // this asserts the canvas followed it. (size + 8 quiet modules) x 5 px.
    const expect = (text) => (SHIPPED_QR.encode(text).size + 8) * 5;
    assertEqual(drawn.pair.width, expect(drawn.pairUrl), "the pairing code's canvas is the wrong size");
    assert(drawn.pair.width === drawn.pair.height, "the pairing code is not square");
    // THE ADDRESS IS WRITTEN OUT BESIDE THE CODE. A code and nothing else is a screen that
    // cannot be used by anyone whose camera will not focus, and it is the only way to tell what
    // the code points at without scanning it.
    //
    // AND IT IS A TAILNET NAME. A `.local` address here would be the removed path's origin
    // coming back through the one door §61 leaves open.
    assert(
      /^https:\/\/[a-z0-9.-]+\.ts\.net:8443\/#pair=[A-Z0-9]{8}$/.test(drawn.pairUrl),
      "the pairing address is not a tailnet name: " + drawn.pairUrl
    );
    return (
      "one code, drawn and square at " + drawn.pair.width + "px — (size + 8 quiet modules) x 5 " +
      "for the version its own URL needs — with " + drawn.pairUrl + " written out beside it"
    );
  });

  // ---- 3. THE PIXELS — the debt check 14's exclusion is paid with ------------------------

  await run.check("3  the QR code is measured FROM THE PIXELS: 21:1, and a white quiet zone on all four edges", async () => {
    // WHY FROM THE PIXELS AND NOT FROM THE SOURCE. `phone.js` sets `fillStyle` twice and a
    // reader can see both values, which proves what the code SAYS. It does not prove what WebKit
    // painted — a canvas with a transparent background over a dark page reads as dark modules on
    // dark, and the source looks identical either way. So this reads the surface back.
    const results = [];
    for (const theme of ["dark", "light"]) {
      const page = await openSheet(browser, theme);
      const measured = await page.evaluate(() => {
        function look(id) {
          const canvas = document.getElementById(id);
          const paper = canvas.getContext("2d");
          const { data, width, height } = paper.getImageData(0, 0, canvas.width, canvas.height);
          const seen = new Map();
          for (let i = 0; i < data.length; i += 4) {
            const key = data[i] + "," + data[i + 1] + "," + data[i + 2] + "," + data[i + 3];
            seen.set(key, (seen.get(key) || 0) + 1);
          }
          // The quiet zone: four modules at five pixels each is twenty pixels of margin, and it
          // has to be white on ALL FOUR edges or the code does not scan against a dark page.
          const at = (x, y) => {
            const i = (y * width + x) * 4;
            return [data[i], data[i + 1], data[i + 2], data[i + 3]].join(",");
          };
          const mid = Math.floor(width / 2);
          const edges = {
            top: at(mid, 2),
            bottom: at(mid, height - 3),
            left: at(2, mid),
            right: at(width - 3, mid),
            corner: at(1, 1),
          };
          return { colors: [...seen.keys()].sort(), edges, width, height };
        }
        return { pair: look("phone-qr-pair") };
      });
      await page.close();

      for (const [which, m] of Object.entries(measured)) {
        assertEqual(
          m.colors,
          ["0,0,0,255", "255,255,255,255"],
          which + " in " + theme + " painted something other than opaque black and opaque white. " +
            "A transparent pixel here is a module that takes the page's color, and on a dark page " +
            "that is a code that does not scan"
        );
        for (const [edge, color] of Object.entries(m.edges)) {
          assertEqual(
            color,
            "255,255,255,255",
            which + " in " + theme + ": the quiet zone is not white at the " + edge +
              " edge. The quiet zone is part of the symbol, not decoration — a code drawn flush " +
              "to the page background does not scan"
          );
        }
        results.push(which + "/" + theme);
      }

      // THE PICTURE'S OWN PAGE, with the countdown pinned — see `SHOT_SECONDS_LEFT`. The page
      // above, which READS the canvas back, keeps the running clock: the QR is not drawn from
      // the countdown and the measurement must not be taken through a knob.
      const page2 = await openSheet(browser, theme, {
        phonePairing: true,
        phoneTailnet: READY_TAILNET,
        phonePairingSecondsLeft: SHOT_SECONDS_LEFT,
      });
      const s = await shot(page2, "phone-" + theme, { fullPage: false });
      publishShotFile(s.file, path.join(SHOTS, "phone-" + theme + ".png"));
      await page2.close();
    }

    // The ratio, computed here rather than asserted as a number somebody remembers.
    const lin = (c) => (c / 255 <= 0.03928 ? c / 255 / 12.92 : Math.pow((c / 255 + 0.055) / 1.055, 2.4));
    const lum = (r, g, b) => 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
    const ratio = (1.0 + 0.05) / (lum(0, 0, 0) + 0.05);
    assert(ratio >= 4.5, "black on white is not clearing the floor, which cannot happen: " + ratio);
    return (
      results.length + " canvas/theme combination(s) read back with getImageData: exactly two " +
      "colors present, opaque #000000 and opaque #ffffff, at " + ratio.toFixed(2) + ":1 — and the " +
      "quiet zone white on all four edges plus the corner, in BOTH themes. This is the measurement " +
      "`contrast.js` check 14's exclusion names."
    );
  });

  // ---- 4, 5, 6. the prose, which is the feature -------------------------------------------

  await run.check("4  nothing is installed on the phone, and the screen says so where it matters", async () => {
    // WHAT CHECKS 4, 5 AND 6 USED TO BE, and why they are one check now.
    //
    // They asserted the two warnings this flow opened with — the red "Unverified", and Apple's
    // iOS 18.0/18.1 bug that hides the trust switch — and that the sixteen taps were sixteen and
    // named both hazards where they fall. All three were about a path CEO §61 removed on
    // 2026-09-19: *"any mobile app or PWA is utterly useless within the home network … The
    // Tailscale setup is where we start now."* There is no certificate for the phone, so there
    // is no red word to warn about and no switch to find.
    //
    // WHAT REPLACES THEM IS THE TRIPWIRE, and it is not decoration. A user on this path who is
    // asked to install a profile is on an origin that is not this Mac's, and the only person who
    // can notice that is the person holding the phone. So the screen says outright that nothing
    // is installed — which is both the good news and the thing that makes the bad news legible.
    const page = await openSheet(browser, "dark");
    const text = (await page.textContent("#phone-pairing")).replace(/\s+/g, " ");
    const gone = await page.evaluate(() => ({
      trustCanvas: !!document.getElementById("phone-qr-trust"),
      trustUrl: !!document.getElementById("phone-trust-url"),
      sixteenSteps: !!document.getElementById("phone-steps"),
      homeWarnings: !!document.getElementById("phone-home-warnings"),
    }));
    await page.close();
    assert(
      /Nothing has to be installed on your phone/i.test(text),
      "the screen does not say that nothing is installed: " + JSON.stringify(text.slice(0, 400))
    );
    assert(
      /asks you to install a profile, something is wrong/i.test(text),
      "the tripwire is gone — a user asked for a profile has nothing telling him that is wrong"
    );
    assert(
      !/Unverified/i.test(text) && !/iOS 18/i.test(text),
      "a warning about the removed path's red word is still on the screen: " + JSON.stringify(text.slice(0, 400))
    );
    for (const [what, present] of Object.entries(gone)) {
      assert(!present, "the removed path's node `" + what + "` is still in the markup");
    }
    return (
      "the pairing screen says nothing is installed and names the one thing that would mean " +
      "something is wrong; the four nodes the removed path needed are not in the markup"
    );
  });

  await run.check("5  the sheet opens on the Tailscale flow, with no question in front of it", async () => {
    // CEO §61, and the defect this check exists to keep out. The flow used to open on *"Where do
    // you want to use it?"* with two options, the second of which led to a self-signed
    // certificate and sixteen taps. §61 does not keep that as a lesser path — it says the thing
    // it does is useless — so the entry point is the first screen of the one path there is.
    //
    // THREE MACS, because the entry point depends on what detection finds and on nothing else:
    // no Tailscale at all, Tailscale with no account this Mac can name, and a ready Mac.
    const cases = [
      {
        what: "a Mac with no Tailscale",
        tailnet: { state: "absent" },
        // No account is known, so §61.1's identity warning is what he meets.
        expect: "phone-identity",
      },
      {
        what: "a Mac signed in but with no name yet",
        tailnet: { state: "certificates-off", account: "Google as someone@gmail.com" },
        expect: "phone-ts-wait",
      },
      {
        what: "a ready Mac",
        tailnet: {
          state: "ready",
          name: "mm1.tail9a3b2.ts.net",
          origin: "https://mm1.tail9a3b2.ts.net:8443",
          account: "Google as someone@gmail.com",
        },
        expect: "phone-ts-ready",
      },
    ];
    const seen = [];
    for (const c of cases) {
      const page = await openSheet(browser, "dark", { phoneTailnet: c.tailnet });
      const state = await page.evaluate(() => {
        const vis = (id) => {
          const node = document.getElementById(id);
          return !!node && !node.hidden;
        };
        return {
          chooser: !!document.getElementById("phone-route"),
          offScreen: !!document.getElementById("phone-off"),
          identity: vis("phone-identity"),
          wait: vis("phone-ts-wait"),
          ready: vis("phone-ts-ready"),
          // Every control on the sheet, so a chooser cannot come back under another id.
          labels: [...document.querySelectorAll("#phone-sheet button")]
            .filter((b) => b.offsetParent !== null)
            .map((b) => b.textContent.trim()),
        };
      });
      await page.close();
      assert(!state.chooser, c.what + ": the route chooser is back in the markup");
      assert(!state.offScreen, c.what + ": the removed path's own screen is back in the markup");
      assert(
        state[{ "phone-identity": "identity", "phone-ts-wait": "wait", "phone-ts-ready": "ready" }[c.expect]],
        c.what + " did not open on " + c.expect + ": " + JSON.stringify(state)
      );
      for (const label of state.labels) {
        assert(
          !/at home/i.test(label) && !/different way/i.test(label),
          c.what + ": a control still offers a second path or a way back to a chooser: " +
            JSON.stringify(label)
        );
      }
      seen.push(c.what + " -> " + c.expect);
    }
    return "three Macs, three entry points, no question in front of any of them: " + seen.join("; ");
  });

  await run.check("6  the four things to do on the phone are four, and none of them is an install", async () => {
    // The CEO's own steps, and they override Urban's three: he installed and uninstalled
    // Tailscale on Android THREE TIMES looking for a "connect to Mac" step that does not exist.
    // Urban's Screen 5 stopped at "sign in", which is where that hunt begins. The VPN prompt and
    // the switch reading Connected are what end it.
    const page = await openSheet(browser, "dark");
    const steps = await page.$$eval("#phone-ts-steps ol.phone-steps li", (nodes) =>
      nodes.map((n) => n.textContent.replace(/\s+/g, " ").trim())
    );
    const text = (await page.textContent("#phone-ts-steps")).replace(/\s+/g, " ");
    await page.close();
    assertEqual(steps.length, 4, "the four phone steps are not four: " + JSON.stringify(steps));
    assert(/Install Tailscale from the store/.test(steps[0]), steps[0]);
    assert(/same/i.test(steps[1]) && /network/i.test(steps[1]), steps[1]);
    assert(/VPN/.test(steps[2]), steps[2]);
    assert(/Connected/.test(steps[3]), steps[3]);
    assert(
      /There is no pairing step in Tailscale/.test(text),
      "the sentence that ends the hunt for a pairing step is gone"
    );
    for (const step of steps) {
      assert(
        !/profile|certificate|passcode|Certificate Trust/i.test(step),
        "a phone step asks for something to be installed on the phone: " + JSON.stringify(step)
      );
    }
    return "four steps, ending at the switch that says Connected, and not one of them installs anything on the phone";
  });

  // ---- 7. the six words, and the countdown -------------------------------------------------

  await run.check("7  no words beside the code; six words and the two answers once a phone arrives", async () => {
    // SAGE'S PAIRING REVIEW, 3.1 STEPS 1 AND 4. The six words used to be printed beside the code
    // before any phone had arrived, so they carried nothing the code does not: anybody who could
    // see the code could see them (F1's evidence). They are derived over the key a phone
    // registered now, so the pairing screen has none, and says where they will appear and which
    // buttons answer them.
    const page = await openSheet(browser, "dark");
    const onCode = await page.evaluate(() => ({
      words: !!document.getElementById("phone-words"),
      sheet: document.getElementById("phone-sheet").innerText.replace(/\s+/g, " "),
      note: document.getElementById("phone-words-note").textContent.replace(/\s+/g, " ").trim(),
      noteShown: document.getElementById("phone-words-note").offsetParent !== null,
    }));
    await page.close();
    assert(!onCode.words, "the pairing screen still carries a node for the six words");
    assert(
      !/harbor\s+candle\s+meadow/.test(onCode.sheet),
      "six words are on screen before any phone has reached this Mac: " + onCode.sheet.slice(0, 200)
    );
    assert(onCode.noteShown, "the pairing screen does not say what happens once the phone opens the code");
    for (const control of ["They match", "They do not match"]) {
      assert(onCode.note.includes("press " + control), `the note does not name ${JSON.stringify(control)}: ${onCode.note}`);
    }

    // AND THE CARD A PHONE REACHES: six plain words, and the two controls the note named.
    const waiting = await openSheet(browser, "dark", { phonePaired: true, phoneMacUnconfirmed: true, phoneFingerprintUnconfirmed: true });
    const words = (await waiting.textContent("#phone-paired-words")).trim().split(/\s+/);
    const buttons = await waiting.$$eval("#phone-mac-answer button", (els) =>
      els.filter((e) => e.offsetParent !== null).map((e) => e.textContent.trim()));
    await waiting.close();
    assertEqual(words.length, 6, "the fingerprint is not six words: " + JSON.stringify(words));
    assert(
      words.every((w) => /^[a-z]{3,7}$/.test(w)),
      "a fingerprint word is not three to seven plain lowercase letters: " + JSON.stringify(words)
    );
    assertEqual(JSON.stringify(buttons), JSON.stringify(["They match", "They do not match"]), "the two answers are not on the card");
    return `no words beside the code; the card shows ${words.length} words and ${buttons.join(" / ")}`;
  });

  await run.check("8  a live code says how long is left, and an expired one does not look live", async () => {
    // The state a real person meets most often: he opens the screen, goes to find his phone, and
    // comes back after the sixty seconds are up. A code that has run out and still looks usable is
    // a code he scans and is only then told about.
    const live = await openSheet(browser, "dark", { phonePairing: true });
    const countdown = (await live.textContent("#phone-countdown")).trim();
    const pairVisible = await live.isVisible("#phone-qr-pair");
    await live.close();
    assert(
      // Minutes while there are minutes, seconds in the last one — the window is 300 s now and
      // "This code lasts 287 more seconds." is a number nobody converts.
      /lasts \d+ more (seconds?|minutes?)\./.test(countdown),
      "a live code does not say how long is left: " + JSON.stringify(countdown)
    );
    assert(pairVisible, "a live code is not on screen");

    // **DEFECT 3.3 — AN EXPIRED CODE IS ANNOUNCED WHERE IT EXPIRED, NEVER A SILENT RESET.**
    //
    // What Ray met: the code, the QR and the six words vanished with no message at all and the
    // dialog dropped back to "This Mac is ready". *"He comes back to a screen that looks like
    // he imagined the whole thing."*
    const expired = await openSheet(browser, "dark", { phonePairingExpired: true });
    const shown = await expired.evaluate(() => {
      const visible = (id) => {
        const node = document.getElementById(id);
        return !!node && node.offsetParent !== null;
      };
      return {
        pairingScreen: visible("phone-pairing"),
        expiredBlock: visible("phone-expired"),
        expiredHeading: (document.querySelector("#phone-expired h3") || {}).textContent || "",
        expiredNote: (document.getElementById("phone-expired-note") || {}).textContent || "",
        qrShown: visible("phone-qr-pair"),
        codeShown: (document.getElementById("phone-pair-url").textContent || "").trim(),
        // Sage 3.1: the pairing screen has no words node at all now; both are "nothing shown".
        wordsShown: ((document.getElementById("phone-words") || {}).textContent || "").trim(),
        // The one control the screen is about, and it must be pressable where he is standing.
        newCode: visible("phone-refresh") && !document.getElementById("phone-refresh").disabled,
        newCodeLabel: document.getElementById("phone-refresh").textContent.trim(),
        // The screen he must NOT have been thrown back to. (The route chooser and the other
        // path's own screen were in this list; §61 deleted both, and check 5 is what asserts
        // they are not in the markup at all.)
        readyScreen: visible("phone-ts-ready"),
      };
    });
    await expired.close();
    assert(
      !shown.qrShown && !shown.codeShown && !shown.wordsShown,
      "an expired window still shows a code, so he scans one that cannot work: " + JSON.stringify(shown)
    );
    assert(
      shown.pairingScreen && shown.expiredBlock,
      "the expiry is not announced on the screen it happened on — this is the silent reset: " +
        JSON.stringify(shown)
    );
    assert(
      !shown.readyScreen,
      "an expired code threw him back to an earlier screen: " + JSON.stringify(shown)
    );
    assert(
      /ran out/i.test(shown.expiredHeading) && /Show me another code/.test(shown.expiredNote),
      "the expiry does not say what happened and point at the way on: " +
        JSON.stringify(shown.expiredHeading + " / " + shown.expiredNote)
    );
    assert(
      shown.newCode && /another code/i.test(shown.newCodeLabel),
      "a new code is not offered in its place: " + JSON.stringify(shown)
    );
    return (
      "a live code says " + JSON.stringify(countdown) + "; an expired one stays on the same " +
      "screen, shows no code, says " + JSON.stringify(shown.expiredHeading.trim()) + " and " +
      "offers " + JSON.stringify(shown.newCodeLabel)
    );
  });

  // ---- 8c. the harness's window is the Mac's window -----------------------------------------

  await run.check("8c  the window the harness models is the window the Mac serves", async () => {
    // `PAIRING_WINDOW_MS` went from 60 s to 300 s for defect 3.3. The mock models the countdown
    // itself, so a mock left at 60 s would draw a countdown the product never shows and every
    // check above it would be measuring a screen that does not ship. This is one grep against
    // the Rust rather than a second copy of the number.
    const modRs = fs.readFileSync(
      path.resolve(UI_DIR, "..", "src-tauri", "src", "phone", "mod.rs"),
      "utf8"
    );
    const rust = /pub const PAIRING_WINDOW_MS: u64 = ([\d_]+);/.exec(modRs);
    assert(rust, "PAIRING_WINDOW_MS is not declared in src-tauri/src/phone/mod.rs any more");
    const mockJs = fs.readFileSync(path.join(UI_DIR, "mock.js"), "utf8");
    const mock = /const PAIRING_WINDOW_MS = (\d+);/.exec(mockJs);
    assert(mock, "ui/mock.js no longer declares the window it models");
    const rustMs = Number(rust[1].replace(/_/g, ""));
    assertEqual(
      Number(mock[1]),
      rustMs,
      "the harness models a different pairing window than the Mac serves"
    );
    assert(
      rustMs >= 180000,
      "the window is " + rustMs / 1000 + " s. Defect 3.3 is that sixty seconds is not enough " +
        "time to read the screen, walk to the phone, unlock it and open the camera — a window " +
        "under three minutes puts that defect back"
    );
    return (
      "PAIRING_WINDOW_MS is " + rustMs / 1000 + " s in phone/mod.rs and " + Number(mock[1]) / 1000 +
      " s in ui/mock.js. Pairing attempt and wrong-code behavior are verified by the Rust device tests."
    );
  });

  // ---- 9. the paired state, and the cleanup he has to know about -------------------------

  await run.check("9  once paired, the screen says what he must do on the phone to undo it", async () => {
    // Plan §4.1: "A cleanup the user has to know to do is a cleanup that does not happen" — so the
    // app says it at the moment it becomes true rather than putting it in a README.
    const page = await openSheet(browser, "dark", { phonePaired: true });
    const text = (await page.textContent("#phone-paired")).replace(/\s+/g, " ");
    const heading = (await page.textContent("#phone-device-name")).trim();
    const forget = await page.isVisible("#phone-forget");
    await page.close();
    assert(forget, "there is no way to forget the phone");
    // THE PHONE IS NAMED, AND SINCE URBAN'S G8 IT IS NAMED IN THE HEADING. This read
    // `/iPhone is paired/` over the card's flat text, which is the same property expressed as
    // one sentence — the name and the state ran together in a single paragraph because the card
    // had no heading at all. It does now, so the two halves are asserted where they each live:
    // weakening this to a search over the flat text would have passed on a card with no heading.
    assertEqual(heading, "iPhone", "the paired phone is not named in the card's heading: " + text);
    assert(/is paired/.test(text), "the card does not say the phone is paired: " + text);
    // **WHAT THIS ASSERTION WAS, AND WHAT IT BECAME.** It read
    // `/does not remove the certificate from your phone/` and then required Apple's own
    // `VPN & Device Management` and `Remove Profile` by name, because forgetting a phone that
    // had paired over the removed path left a profile on it that only the user could delete.
    // CEO §61 removes that path: nothing is installed on the phone, so there is nothing left
    // behind and a card that named an iOS menu would be sending him to undo something that was
    // never done. The rule underneath it — plan §4.1's *"a cleanup the user has to know to do is
    // a cleanup that does not happen"* — is unchanged and is what is asserted instead: the card
    // says exactly what forgetting does, and says outright that the phone needs nothing.
    //
    // The MIGRATION case, where a record from an older build means there may be something to
    // remove after all, is check 9b's third and fourth combinations.
    assert(
      /There is nothing to remove from your phone/i.test(text),
      "the card does not say the phone needs nothing done to it: " + text
    );
    assert(
      /deletes the keys/i.test(text) && /stops this Mac answering/i.test(text),
      "the card does not say what forgetting actually does: " + text
    );
    assert(
      !/Remove Profile/.test(text) && !/VPN & Device Management/.test(text),
      "the card sends him to remove something this build never put on his phone: " + text
    );
    return "the paired state names the phone, offers the way out, and says exactly what forgetting does";
  });

  // ---- 9e. DEFECT 2 — the Mac does not answer a question the person is still being asked ----

  await run.check(
    "9e  the Mac says `It is paired` only after the person has answered the six words",
    async () => {
      // RAY'S NIGHTLY `.7`, DEFECT 2, and his frame 13 has both screens on it at once: the Mac's
      // sheet reading `Phone — It is paired. Open Rich on it and keep talking.` while the phone
      // was still asking `They match — pair this phone` / `They do not match`.
      //
      // The six words exist so a person can detect that something other than his Mac answered.
      // A Mac that settles that question before he has answered it teaches him the check is
      // ceremonial — and it had no choice, because nothing in the protocol carried his answer
      // until `POST /api/pair` took `fingerprint_confirmed`.
      //
      // THE STATE UNDER TEST WAS UNDRAWABLE UNTIL NOW, which is why no check caught it: the
      // fixture had no way to be paired-and-unanswered, so every existing check on this card
      // silently modeled the end state.
      //
      // SINCE SAGE'S F1 THE QUESTION IS PUT ON THIS MAC, and the sentence names the button here:
      // the phone's own answer was the only one the Mac recorded, and it was the device being
      // judged that gave it.
      const waiting = await openSheet(browser, "dark", {
        phonePaired: true,
        phoneFingerprintUnconfirmed: true,
        phoneMacUnconfirmed: true,
      });
      const before = (await waiting.textContent("#phone-paired")).replace(/\s+/g, " ");
      const wordsShown = await waiting.isVisible("#phone-paired-words");
      const words = (await waiting.textContent("#phone-paired-words")).trim();
      const forgetShown = await waiting.isVisible("#phone-forget");
      await waiting.close();

      assert(
        !/It is paired/.test(before),
        "the Mac claims the phone is paired before the person has answered the six words: " + before
      );
      assert(
        /six words/i.test(before) && /press They match/i.test(before),
        "the card does not say what he is being asked to do here: " + before
      );
      assert(!forgetShown, "`Forget this phone` is offered for a phone that is not paired yet");
      // AND THE MAC'S HALF OF THE COMPARISON IS ON THE SCREEN WHILE HE IS MAKING IT. The pairing
      // screen that carries the words is hidden the instant `paired` flips, which is the same
      // instant the phone starts asking — so without this the Mac took its own six words away at
      // exactly the moment they were needed.
      assert(wordsShown, "the six words are not on the Mac while the phone is asking about them");
      assertEqual(
        words,
        "harbor  candle  meadow  lantern  fossil  juniper",
        "the words on the paired card are not the Mac's own fingerprint"
      );

      // And the finish: once the phone has come back, the sentence is the one it always was and
      // the words are gone — a fingerprint nobody is checking is chrome.
      const done = await openSheet(browser, "dark", { phonePaired: true });
      const after = (await done.textContent("#phone-paired")).replace(/\s+/g, " ");
      const stillThere = await done.isVisible("#phone-paired-words");
      const answersThere = await done.isVisible("#phone-mac-answer");
      await done.close();
      assert(/It is paired/.test(after), "a confirmed phone is not called paired: " + after);
      assert(!stillThere, "the six words stayed on the card after he had answered them");
      assert(!answersThere, "the two answers stayed on the card after he had answered");

      // BETWEEN THE TWO: this Mac pressed, the phone has not answered yet.
      const half = await openSheet(browser, "dark", { phonePaired: true, phoneFingerprintUnconfirmed: true });
      const halfway = (await half.textContent("#phone-paired-state")).trim();
      const halfWords = await half.isVisible("#phone-paired-words");
      await half.close();
      assert(!/It is paired/.test(halfway) && /on your phone/i.test(halfway), "the card after the Mac's press: " + halfway);
      assert(!halfWords, "the six words stayed on the Mac after the Mac had answered");

      // SAGE'S PAIR-V2 REVIEW §2: BOTH PRESSES ARE IN AND THE PHONE HAS NOT USED THE PAIRING. On
      // `main` this read `It is paired` — for a phone that may already have thrown its key away.
      const unused = await openSheet(browser, "dark", { phonePaired: true, phoneUnfinished: true });
      const unusedLine = (await unused.textContent("#phone-paired-state")).trim();
      const unusedWords = await unused.isVisible("#phone-paired-words");
      await unused.close();
      assertEqual(
        unusedLine,
        "This Mac accepted it. Waiting for your phone to finish.",
        "the card claims a phone is paired before the phone has used the pairing"
      );
      assert(!unusedWords, "the six words came back after both answers were in");

      return (
        "unconfirmed: \"" + before.slice(0, 96) + "…\" with the words on screen; " +
        "Mac pressed: \"" + halfway + "\"; phone not finished: \"" + unusedLine + "\"; " +
        "confirmed: \"" + after.slice(0, 48) + "…\" with the words gone"
      );
    }
  );

  // ---- 9g. SAGE F1 — the answer that counts is given on this Mac ----------------------------

  await run.check(
    "9g  `They match` on the Mac lets the phone in, `They do not match` stops it, and both read AA in both themes",
    async () => {
      // Sage's pairing review F1 (High): the Mac accepted every signed request from a phone that
      // had confirmed nothing, and the only confirmation it recorded was the phone's own. The fix
      // is two buttons on this card; this check presses both, measures what they say, and holds
      // that neither is the sheet's first focus.
      const { parseCssColor, compositeOver, contrastRatio, round2, isLargeText, hex } = require("./lib/contrast");
      const measure = (page, selector) =>
        page.evaluate((sel) => {
          const el = document.querySelector(sel);
          const cs = getComputedStyle(el);
          let node = el;
          let ground = "rgba(0, 0, 0, 0)";
          while (node) {
            const bg = getComputedStyle(node).backgroundColor;
            if (bg && !/rgba\([^)]*,\s*0\s*\)/.test(bg) && bg !== "transparent") { ground = bg; break; }
            node = node.parentElement;
          }
          return { color: cs.color, ground, size: parseFloat(cs.fontSize), weight: cs.fontWeight };
        }, selector);
      const out = [];
      for (const theme of ["dark", "light"]) {
        const page = await openSheet(browser, theme, { phonePaired: true, phoneMacUnconfirmed: true, phoneFingerprintUnconfirmed: true });
        await page.waitForSelector("#phone-mac-answer:not([hidden])");
        const focused = await page.evaluate(() => document.activeElement && document.activeElement.id);
        assert(!/^phone-mac-/.test(focused || ""), "the sheet opened with focus on an answer to the six words: " + focused);
        for (const selector of ["#phone-paired-state", "#phone-paired-words", "#phone-mac-match", "#phone-mac-mismatch"]) {
          const m = await measure(page, selector);
          const ink = parseCssColor(m.color);
          const ground = parseCssColor(m.ground);
          const composited = ink.a < 1 ? compositeOver(ink, ground) : ink;
          const ratio = round2(contrastRatio(composited, ground));
          const floor = isLargeText(m.size, m.weight) ? 3 : 4.5;
          assert(m.size >= 16, `${selector} is ${m.size}px in ${theme}, under the 16px floor for readable text`);
          assert(ratio >= floor, `${selector} is ${ratio}:1 in ${theme} (${hex(composited)} on ${hex(ground)}) against ${floor}:1`);
          out.push(`${theme} ${selector} ${ratio}:1`);
        }
        await page.close();
      }

      // "They match": the card moves to the next true sentence and the question leaves it.
      const yes = await openSheet(browser, "dark", { phonePaired: true, phoneMacUnconfirmed: true, phoneFingerprintUnconfirmed: true });
      await yes.click("#phone-mac-match");
      await yes.waitForFunction(() => document.getElementById("phone-mac-answer").hidden, null, { timeout: 5000 });
      const afterYes = (await yes.textContent("#phone-paired-state")).trim();
      await yes.close();
      assert(/This Mac accepted it/.test(afterYes), "after `They match` the card says: " + afterYes);

      // "They do not match": the Mac stops, and says that it was HIM, here, in one sentence.
      const no = await openSheet(browser, "dark", { phonePaired: true, phoneMacUnconfirmed: true });
      await no.click("#phone-mac-mismatch");
      await no.waitForSelector("#phone-rejected:not([hidden])");
      const note = (await no.textContent("#phone-rejected-note")).trim();
      await no.close();
      assert(/^You said the six words did not match/.test(note), "the Mac's refusal is reported as someone else's: " + note);
      assertEqual((note.match(/\.\s|\.$/g) || []).length, 1, "the report is more than one sentence: " + note);

      // And every other reason the Mac can stop gets its own true sentence, in the same shape.
      for (const [by, needle] of [["code-used-twice", /second phone used the same code/], ["expired", /within five minutes/], ["unfinished", /never finished pairing/], ["phone", /^Your phone said/]]) {
        const page = await openSheet(browser, "dark", { phoneRejected: true, phoneRejectedBy: by, phoneTailnet: READY_TAILNET });
        await page.waitForSelector("#phone-rejected:not([hidden])");
        const said = (await page.textContent("#phone-rejected-note")).trim();
        await page.close();
        assert(needle.test(said) && /stopped answering/.test(said), `${by}: ${said}`);
        assertEqual((said.match(/\.\s|\.$/g) || []).length, 1, `${by}: more than one sentence: ${said}`);
      }
      return out.join("; ");
    }
  );

  await run.check("9h  a second use of the code after the press keeps the phone and says so", async () => {
    const page = await openSheet(browser, "dark", { phonePaired: true, phoneCodeReused: true });
    const line = await page.isVisible("#phone-code-reused");
    const text = (await page.textContent("#phone-code-reused")).trim();
    const forget = await page.isVisible("#phone-forget");
    await page.close();
    assert(line && /Forget this phone/.test(text), "the warning is not on the card: " + text);
    assert(forget, "the warning names `Forget this phone` and the button is not there");
    const quiet = await openSheet(browser, "dark", { phonePaired: true });
    const none = await quiet.isVisible("#phone-code-reused");
    await quiet.close();
    assert(!none, "the warning shows on a pairing nobody else touched");
    return text;
  });

  // ---- 9f. DEFECT 1 — the Mac shows that it heard the alarm button ------------------------

  await run.check(
    "9f  `They do not match` leaves the Mac saying what it did, in one sentence, AA in both themes",
    async () => {
      // RAY'S NIGHTLY `.8` WALK IN THE TEST VM, DEFECT 1 (HIGH). He pressed `They do not match`
      // on the phone; the credential really was dropped, and the sheet in front of him did not
      // change at all. The card still read `It has reached this Mac. Check that the six words on
      // it are the six words below…` with the six words underneath, and the Mac went on answering
      // `curl` on 8443 at t+10, 20, 30, 40, 50 and 60 s and two minutes later. *"He has no way to
      // know the Mac heard him."*
      //
      // The socket half is proved over real TLS in
      // `phone::listen::tests::they_do_not_match_over_the_wire_stops_the_listener`. This is the
      // other half: what the person is looking at.
      //
      // **THE HARD PART OF THIS STATE IS THAT IT LOOKS LIKE SUCCESS.** A Mac that has stopped is
      // not listening, has nothing paired and has no code — which is character for character the
      // status that draws `This Mac is ready`. So the first two assertions are that the ready
      // screen and the paired card are BOTH off: without them a green here would be a sheet that
      // had quietly gone back to the beginning.
      const { parseCssColor, compositeOver, contrastRatio, round2, isLargeText, hex } =
        require("./lib/contrast");
      const out = [];
      let sentence = "";
      for (const theme of ["dark", "light"]) {
        const page = await openSheet(browser, theme, {
          phoneRejected: true,
          phoneTailnet: READY_TAILNET,
        });
        await page.waitForSelector("#phone-rejected:not([hidden])");

        const seen = await page.evaluate(() => {
          const vis = (id) => {
            const el = document.getElementById(id);
            return !!(el && !el.hidden && el.offsetParent !== null);
          };
          return {
            note: document.getElementById("phone-rejected-note").textContent.trim(),
            heading: document.querySelector("#phone-rejected .phone-step-title").textContent.trim(),
            again: document.getElementById("phone-rejected-again").textContent.trim(),
            againUsable:
              vis("phone-rejected-again") &&
              !document.getElementById("phone-rejected-again").disabled,
            ready: vis("phone-ts-ready"),
            paired: vis("phone-paired"),
            pairing: vis("phone-pairing"),
            words: vis("phone-words") || vis("phone-paired-words"),
            closeThere: vis("phone-close"),
          };
        });

        assert(!seen.ready, "the sheet went back to `This Mac is ready` as though nothing happened");
        assert(!seen.paired, "the paired card is still up after the phone was forgotten");
        assert(!seen.pairing, "the pairing screen is still up after the Mac stopped");
        assert(
          !seen.words,
          "the six words are still on screen after the person said they did not match"
        );
        assert(
          /did not match/.test(seen.note),
          "the Mac does not say what the phone told it: " + seen.note
        );
        assert(
          /stopped answering/.test(seen.note) &&
            /forgot the phone/.test(seen.note) &&
            /deleted the certificate/.test(seen.note),
          "the Mac does not say what it DID about it, which is the half he cannot see: " + seen.note
        );
        // ONE SENTENCE. Ray's ask, and the reason this is a screen rather than a paragraph.
        assertEqual(
          (seen.note.match(/\.\s|\.$/g) || []).length,
          1,
          "the report is more than one sentence: " + seen.note
        );
        // AND A WAY OFF IT. The flag lives on the Mac and only `phone_begin_pairing` clears it,
        // so a screen without this control would be a screen he cannot leave.
        assert(seen.againUsable, "the screen that says the Mac stopped has no way forward");
        assert(seen.closeThere, "Close is not on the screen");

        // THE CONTRAST, COMPUTED FROM THE RENDERED STYLES RATHER THAN EYEBALLED. Both themes,
        // 4.5:1 for this size — it is a sentence a person is expected to read, so no exemption
        // is available and none is claimed.
        const measured = await page.evaluate(() => {
          const el = document.getElementById("phone-rejected-note");
          const cs = getComputedStyle(el);
          let node = el;
          let ground = "rgba(0, 0, 0, 0)";
          while (node) {
            const bg = getComputedStyle(node).backgroundColor;
            if (bg && !/rgba\([^)]*,\s*0\s*\)/.test(bg) && bg !== "transparent") {
              ground = bg;
              break;
            }
            node = node.parentElement;
          }
          return {
            color: cs.color,
            ground,
            size: parseFloat(cs.fontSize),
            weight: cs.fontWeight,
          };
        });
        const ink = parseCssColor(measured.color);
        const ground = parseCssColor(measured.ground);
        const composited = ink.a < 1 ? compositeOver(ink, ground) : ink;
        const ratio = round2(contrastRatio(composited, ground));
        const floor = isLargeText(measured.size, measured.weight) ? 3 : 4.5;
        assert(
          ratio >= floor,
          "the sentence that tells him the Mac heard him is " + ratio + ":1 in " + theme +
            " (" + hex(composited) + " on " + hex(ground) + ") against a " + floor + ":1 floor"
        );
        out.push(
          theme + " " + ratio + ":1 (" + hex(composited) + " on " + hex(ground) + ", " +
            measured.size + "px)"
        );
        sentence = seen.note;
        await page.close();
      }

      // AND THE CONTROL THAT LEAVES IT REALLY LEAVES IT. Pressing it asks the Mac to pair again,
      // which is the one call that clears the flag — the harness answers with a Mac that is no
      // longer rejected, so the screen must go.
      const page = await openSheet(browser, "dark", {
        phoneRejected: true,
        phoneTailnet: READY_TAILNET,
      });
      await page.waitForSelector("#phone-rejected:not([hidden])");
      // Nothing is poked: the harness clears its own flag inside `phone_begin_pairing`, which
      // is where the Mac clears it, so this press exercises the real transition.
      await page.click("#phone-rejected-again");
      // `waitForSelector` defaults to waiting for VISIBLE, so a hidden node is asked for as a
      // predicate rather than a selector.
      await page.waitForFunction(() => document.getElementById("phone-rejected").hidden, null, {
        timeout: 5000,
      });
      const left = await page.isVisible("#phone-pairing");
      await page.close();
      assert(left, "`Set my phone up again` did not reach the pairing screen");

      return '"' + sentence + '" — ' + out.join("; ");
    }
  );

  // ---- 9d. ROUND 49 — the paired card after the phone web page is gone ---------------------

  await run.check(
    "9d  round 49: each paired-card state says its own words for both phones, none of the retired ones, its marks clear 3:1",
    async () => {
      // CEO, 2026-10-31: "round-49/index.html: OK" (richos-hq design/mockups/rounds/round-49,
      // README "The states"). The phone web page is retired (CEO, 2026-10-30), so the card names
      // the RichConnect app and nothing else. On main a RichConnect Android phone ("fcm") was told
      // the Chrome sentence whatever it had done, because push_ready read only the retired web
      // subscription, and an iPhone ("apns") was pointed at "the RichOS iPhone app". The words
      // below are round 49's, character for character; the same sentences serve both phones.
      const { measureIndicatorCrop } = require("./lib/contrast");
      const PAIRED = "It is paired. Open RichConnect on it and keep talking.";
      const WORDS = {
        ready: {
          chip: "Connected", tone: "ok", state: PAIRED,
          line: "It can reach you with a notification when Rich has something for you. You can switch that off in RichConnect on the phone, under Settings.",
          strong: ["Settings"],
        },
        off: {
          chip: "Connected", tone: "ok", state: PAIRED,
          line: "It cannot send you notifications yet. In RichConnect on the phone, open Settings and switch on Notify me when Rich replies, then allow notifications when the phone asks.",
          strong: ["Settings", "Notify me when Rich replies"],
        },
        away: {
          chip: "Reconnecting", tone: "wait",
          state: "Its pairing is saved. RichOS is trying to restore the connection.",
          line: "There is nothing to do on the phone. It reconnects by itself while this Mac is awake with RichOS open.",
          strong: [],
        },
        old: {
          chip: "Pair again", tone: "todo",
          state: "This phone was paired by an older version of RichOS and cannot reach Rich anymore.",
          line: "To keep using Rich on it, install RichConnect on the phone, then press Forget this phone below and pair it again.",
          strong: ["Forget this phone"],
        },
      };
      // What the Mac's status says in each state (`PhoneStatus`): a RichConnect phone names its
      // push service when it answers They match; the retired page left "web-push".
      const PRESET = {
        ready: (transport) => ({ phonePushTransport: transport, phoneNotificationsOn: true }),
        off: (transport) => ({ phonePushTransport: transport }),
        away: (transport) => ({ phonePushTransport: transport, phoneListenerStopped: true }),
        old: () => ({ phonePushTransport: "web-push" }),
      };
      const PHONES = { ios: "apns", android: "fcm" };
      // Round 49's own walk refuses these four on the card; the last three are what main showed.
      const RETIRED = [/Safari/i, /Chrome/i, /Home Screen/i, /web page/i, /RichOS iPhone app/i, /Share menu/i, /Open Rich on it/];

      const out = [];
      for (const theme of ["dark", "light"]) {
        for (const [platform, transport] of Object.entries(PHONES)) {
          for (const [state, want] of Object.entries(WORDS)) {
            const which = theme + " " + platform + " " + state;
            const page = await openSheet(browser, theme, Object.assign(
              { phonePaired: true, phonePlatform: platform, phonePairedVia: "connect",
                phoneConnect: { enabled: true, phase: "active", health: { state: state === "away" ? "reconnecting" : "connected" } } },
              PRESET[state](transport)
            ));
            await page.waitForFunction(() => {
              const chip = document.getElementById("phone-chip-word");
              return chip && chip.textContent.trim().length > 0;
            });
            await awaitSettled(page);
            const seen = await page.evaluate(() => {
              const text = (id) => document.getElementById(id).textContent.replace(/\s+/g, " ").trim();
              const chip = document.getElementById("phone-chip");
              const line = document.getElementById("phone-push-state");
              return {
                chip: text("phone-chip-word"),
                chipShown: !chip.hidden && chip.getBoundingClientRect().width > 0,
                tone: chip.dataset.tone,
                state: text("phone-paired-state"),
                line: text("phone-push-state"),
                lineShown: !line.hidden,
                lineTone: line.dataset.tone,
                strong: Array.from(line.querySelectorAll("strong")).map((s) => s.textContent),
                icon: !!line.querySelector(".phone-push-icon svg"),
                card: document.getElementById("phone-paired").innerText,
                headFirst: document.getElementById("phone-paired").firstElementChild.contains(document.getElementById("phone-device-name")),
              };
            });
            assertEqual(seen.chip, want.chip, which + ": the chip");
            assert(seen.chipShown, which + ": the chip is not on screen");
            assertEqual(seen.tone, want.tone, which + ": the chip's tone");
            assertEqual(seen.state, want.state, which + ": the pairing sentence");
            assert(seen.lineShown, which + ": the errand line is hidden");
            assertEqual(seen.line, want.line, which + ": the errand line");
            assertEqual(seen.strong, want.strong, which + ": the phone's control names set in strong");
            assert(seen.icon, which + ": the errand line has no icon");
            assert(seen.headFirst, which + ": the phone's name is not at the top of the card");
            for (const pattern of RETIRED) {
              assert(!pattern.test(seen.card), which + ": the card still says " + pattern + ": " + JSON.stringify(seen.card));
            }
            // THE MARKS, MEASURED FROM THE PIXELS (3:1, non-text): the chip's dot and the icon.
            const marks = [];
            for (const sel of ["#phone-chip .phone-chip-dot", "#phone-push-state .phone-push-icon svg"]) {
              const box = await page.evaluate((s) => {
                const el = document.querySelector(s);
                el.scrollIntoView({ block: "center" });
                const r = el.getBoundingClientRect();
                return { x: r.x, y: r.y, w: r.width, h: r.height };
              }, sel);
              await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
              const png = await page.screenshot({
                clip: { x: Math.round(box.x - 3), y: Math.round(box.y - 3), width: Math.round(box.w + 6), height: Math.round(box.h + 6) },
                animations: "disabled",
              });
              const m = measureIndicatorCrop(png, 3);
              assert(!m.unresolvable, which + " " + sel + ": " + m.unresolvable);
              assert(m.ratio >= 3, which + ": " + sel + " is " + m.ratio + ":1 (" + m.ink + " on " + m.ground + "), under the 3:1 floor for a non-text indicator");
              marks.push(m.ratio);
            }
            await page.close();
            out.push(which + " dot " + marks[0] + ":1 icon " + marks[1] + ":1");
          }
        }
      }
      // AN ORDINARY NETWORK BLIP: RichConnect reconnecting while this Mac's listener is still up.
      // The line above the card says "Reconnecting this Mac", so the chip must not say Connected.
      for (const platform of Object.keys(PHONES)) {
        const page = await openSheet(browser, "dark", {
          phonePaired: true, phonePlatform: platform, phonePairedVia: "connect", phoneNotificationsOn: true,
          phoneConnect: { enabled: true, phase: "active", health: { state: "reconnecting" } },
        });
        await page.waitForFunction(() => {
          const chip = document.getElementById("phone-chip-word");
          return chip && chip.textContent.trim().length > 0;
        });
        const seen = await page.evaluate(() => ({
          chip: document.getElementById("phone-chip-word").textContent.trim(),
          state: document.getElementById("phone-paired-state").textContent.trim(),
          line: document.getElementById("phone-push-state").textContent.replace(/\s+/g, " ").trim(),
        }));
        await page.close();
        assertEqual(seen.chip, WORDS.away.chip, platform + " reconnecting, listener up: the chip");
        assertEqual(seen.state, WORDS.away.state, platform + " reconnecting, listener up: the pairing sentence");
        assertEqual(seen.line, WORDS.away.line, platform + " reconnecting, listener up: the errand line");
        out.push(platform + " reconnecting with the listener up: " + seen.chip);
      }
      return out.length + " cards in round 49's words, no retired word on any; " + out.join("; ");
    }
  );

  // ---- 9c. DEFECT 3.1 — the first sentence is true of the path the product has -------------

  await run.check(
    "9c  the lead paragraph is true of the one path, on every screen of it",
    async () => {
      // RAY'S CANDIDATE .11 DEFECT 3.1. Every screen opened with "Your phone talks to this Mac
      // directly, over your own home network ... there is no account to make" — a description
      // of the option he had NOT chosen — and it stayed there unchanged after he pressed the
      // other one. Screenshots 13, 14, 15, 17.
      //
      // THE FIX AT THE TIME WAS TO KEY THE SENTENCE OFF THE PATH. CEO §61 removes the second
      // path, so the key goes and one sentence is left — and this check moves with it: a lead
      // that changes between screens is now the defect, not the fix.
      const seen = [];
      const cases = [
        { what: "identity", tailnet: { state: "absent" }, screen: "#phone-identity" },
        {
          what: "waiting",
          tailnet: { state: "certificates-off", account: "Google as someone@gmail.com" },
          screen: "#phone-ts-wait",
        },
        { what: "ready", tailnet: READY_TAILNET, screen: "#phone-ts-ready" },
      ];
      let first = null;
      for (const c of cases) {
        const page = await openSheet(browser, "dark", { phoneTailnet: c.tailnet });
        await page.waitForSelector(c.screen + ":not([hidden])");
        // THE FIRST PARAGRAPH OF THE PANEL, by position rather than by id — the only direct
        // `p.overlay-note` child of the sheet's scroll box — `.overlay-panel` until Ray's
        // candidate-.16 defect put the action row outside the scroll, `#phone-scroll` since.
        // So this check measures the SENTENCE, and a build where that sentence is a fixed
        // literal fails on what it says rather than on a missing element.
        const lead = (await page.textContent("#phone-sheet #phone-scroll > p.overlay-note"))
          .replace(/\s+/g, " ")
          .trim();
        await page.close();
        assert(
          /Tailscale/.test(lead) && /account/.test(lead),
          c.what + ": the lead does not name the network or the account it costs: " + JSON.stringify(lead)
        );
        assert(
          !/home network/i.test(lead) && !/no account to make/i.test(lead),
          "THE DEFECT: the lead still describes the removed path, which is not the network he is " +
            "on and is not true of the account he has to make: " + JSON.stringify(lead)
        );
        if (first === null) first = lead;
        assertEqual(
          lead,
          first,
          "the lead changed between screens. With one path there is nothing for it to follow, so " +
            "a lead that moves is a lead keyed off something that is not the product"
        );
        seen.push(c.what);
      }

      // ONE WORD FOR THE PLACE, and the surviving half of that finding. The flow said "the
      // office", "the house" and "home" in three adjacent sentences. Two of the three sentences
      // went with the route chooser; this keeps the stray words out of what is left.
      const page = await openSheet(browser, "dark");
      const whole = (await page.textContent("#phone-sheet")).replace(/\s+/g, " ");
      await page.close();
      const strays = ["the office", "the house", "home network"].filter((w) => whole.includes(w));
      assertEqual(
        strays,
        [],
        "the flow names a place it has no business naming — §61 is that the phone is for being " +
          "away from one, not that the app has an opinion about which"
      );
      return (
        seen.length + " screens, one lead: " + JSON.stringify(first.slice(0, 64) + "…") +
        " — and 0 stray words for the place across the whole sheet, with every screen's markup " +
        "in the DOM."
      );
    }
  );

  // ---- 9b. DEFECT 3.2 — the card says the same thing on the second open as on the first ----

  await run.check(
    "9b  the paired card's copy comes from the record: this build's path and an older one, identical on reopen",
    async () => {
      // RAY'S CANDIDATE .11 DEFECT 3.2, REPRODUCED AND THEN PINNED.
      //
      // What he did: paired his ANDROID phone over Tailscale, read "There is nothing to remove
      // from your phone", closed the card, reopened it, and was told to open "Settings, then
      // General, then VPN and Device Management, then the RichOS profile" — an iOS menu, on an
      // Android phone, for a certificate that path never installs. Reproduced twice.
      //
      // The cause is that the copy was keyed off the sheet's `route`, which `open()` resets to
      // null on every open by design. So the SECOND OPEN is the whole test: same status, same
      // phone, and the card must say the same words.
      // **TWO ANSWERS NOW, NOT FIVE** (CEO §61). The five were four combinations of path and
      // platform plus a legacy record; with one way to pair, what the card says about a phone
      // paired TODAY does not depend on the phone at all — nothing was put on it. What is left
      // is that answer and the migration sentence for a record an older build wrote, which
      // still has to say something true: pair it again, and the old build may have left a
      // profile on the phone. Both platforms are still walked against the one answer, because
      // a card that grew a platform branch again would be defect 3.2 coming back.
      const combinations = [
        {
          via: "tailnet",
          platform: "android",
          name: "Android phone",
          must: [/nothing to remove from your phone/i, /Tailscale name/],
          mustNot: [/VPN & Device Management/, /Remove Profile/, /Settings, then General/],
          limit: true,
        },
        {
          via: "tailnet",
          platform: "ios",
          name: "iPhone",
          must: [/nothing to remove from your phone/i],
          mustNot: [/VPN & Device Management/, /Remove Profile/, /Settings, then General/],
          limit: true,
        },
        {
          // A RECORD FROM A BUILD WITH A SECOND PATH — including one written before the field
          // existed, which reads the same way here and is told the same true thing. It must not
          // fall back to the answer that is true of a phone paired today: the defect was a
          // default stated as a fact, and a default in here would be the same defect one layer
          // down.
          via: "home",
          platform: "ios",
          name: "iPhone",
          must: [/older version of RichOS/i, /pair it again/i],
          mustNot: [/nothing to remove from your phone/i, /Remove Profile/],
          limit: false,
        },
        {
          via: "",
          platform: "",
          name: "iPhone",
          must: [/older version of RichOS/i, /pair it again/i],
          mustNot: [/nothing to remove from your phone/i, /Remove Profile/],
          limit: false,
        },
      ];

      const seen = [];
      for (const combination of combinations) {
        const page = await openSheet(browser, "dark", {
          phonePaired: true,
          phonePairedVia: combination.via,
          phonePlatform: combination.platform,
        });
        // WHAT IS ON THE SCREEN, NOT WHAT IS IN THE MARKUP. The forget note is read by
        // gathering every VISIBLE paragraph in the paired card that is about forgetting,
        // rather than by one element id — so this check measures the copy a person reads and
        // keeps measuring it whether that copy lives in one node or in two. Run against the
        // build Ray walked it fails on the words, which is the defect, rather than on a
        // missing id, which would only be a rename.
        const read = async () =>
          page.evaluate(() => {
            const visible = (node) => node.offsetParent !== null || !node.hidden;
            const notes = [...document.querySelectorAll("#phone-paired p")]
              .filter((n) => visible(n) && /Forgetting it here/.test(n.textContent || ""))
              .map((n) => (n.textContent || "").replace(/\s+/g, " ").trim());
            const limit = [...document.querySelectorAll("#phone-paired p")].filter(
              (n) => visible(n) && /signed in to Tailscale/.test(n.textContent || "")
            );
            return {
              // SINCE URBAN'S G8 THIS CARD HAS A REAL HEADING, so the name is read off it
              // rather than off the first 40 characters of the card's flat text. The old slice
              // was a stand-in for a heading that did not exist; now that one does, a slice
              // would pass on a card whose name had moved anywhere in the first line.
              heading: document.getElementById("phone-device-name").textContent.trim(),
              notes: notes,
              limitShown: limit.length > 0,
            };
          });
        const first = await read();
        // CLOSE AND REOPEN — the exact sequence that produced the defect on screen.
        await page.click("#phone-close");
        // `state: "hidden"` — the default waits for VISIBLE, and a hidden sheet never is.
        await page.waitForSelector("#phone-sheet", { state: "hidden" });
        await page.click("#set-btn");
        await page.click("#set-phone-open");
        await page.waitForSelector("#phone-sheet:not([hidden])");
        const second = await read();
        await page.close();

        const where = combination.via + ":" + combination.platform;
        assertEqual(
          second,
          first,
          "reopening the paired card for the SAME phone (" + where + ") changed what it says. " +
            "That is defect 3.2: the copy is being derived from something the sheet forgets on " +
            "open instead of from the device record."
        );
        // EXACTLY ONE forget paragraph may be on screen. Two visible paragraphs about the
        // same act is its own defect and the reason this is one node now.
        assertEqual(
          first.notes.length,
          1,
          where + " shows " + first.notes.length + " visible paragraph(s) about forgetting the " +
            "phone, and exactly one is the right number: " + JSON.stringify(first.notes)
        );
        const note = first.notes[0];
        for (const pattern of combination.must) {
          assert(
            pattern.test(note),
            where + " is missing " + pattern + " — it says: " + JSON.stringify(note)
          );
        }
        for (const pattern of combination.mustNot) {
          assert(
            !pattern.test(note),
            where + " shows copy that belongs to another combination (" + pattern + "): " +
              JSON.stringify(note)
          );
        }
        // The name comes last of the three, deliberately: it is the one assertion that
        // depends on the harness modeling the right phone, and a failure there would mask the
        // copy failures above, which are the defect.
        assertEqual(
          first.heading,
          combination.name,
          "the card does not name the phone it is talking about (" + where + "): " + first.heading
        );
        assertEqual(
          first.limitShown,
          combination.limit,
          "the Tailscale limit line is on the wrong combination (" + where + ")"
        );
        seen.push(where + (first.limitShown ? " +limit" : ""));
      }
      return (
        combinations.length + " path x platform combination(s) read off the device record and " +
        "identical on a second open: " + seen.join(", ") + ". The reopen is the measurement — " +
        "before this, every reopen drew the home path's iOS profile-removal steps whatever the " +
        "phone was."
      );
    }
  );

  // ---- 11-16. CEO §61.1: the identity trap, made crystal-clear -----------------------------
  //
  // His ruling, verbatim: *"this barrier or I would call it stupidity would need to be made
  // ABSOLUTELY UBER MEGA SUPER CRYSTAL-CLEAR to ever user of RichOS"*. What he hit on his own
  // devices: Tailscale has no email-and-password sign-in; the account IS the network; he signed in
  // with Apple on the Mac and Google on the Android, got two networks that cannot see each other,
  // and reinstalled Tailscale on the Android THREE TIMES looking for a "connect to Mac" step that
  // does not exist.
  //
  // None of the four checks below could have been written before this run: `mock.js` reported no
  // `tailnet` at all, so every Tailscale screen was unreachable in a browser.

  /// Open the sheet on the flow, which since CEO §61 is the only flow there is.
  ///
  /// It used to click through the route chooser; there is no chooser. What it still does is read
  /// the identity screen when one is up, because every screen after it is BEHIND it — which is
  /// the property check 11 is about, and the reason every other check has to pass through here.
  async function openTheFlow(theme, tailnet) {
    const page = await openSheet(browser, theme, { phoneTailnet: tailnet });
    if (await page.isVisible("#phone-identity")) await page.click("#phone-identity-ok");
    return page;
  }

  await run.check("11  the identity trap is stated BEFORE the download step, and it states all five things", async () => {
    const page = await openSheet(browser, "dark", { phoneTailnet: { state: "absent" } });
    const shown = await page.evaluate(() => ({
      identity: !document.getElementById("phone-identity").hidden,
      wait: !document.getElementById("phone-ts-wait").hidden,
      text: document.getElementById("phone-identity").textContent.replace(/\s+/g, " "),
      heading: document.querySelector("#phone-identity .phone-step-title").textContent,
    }));
    // FIRST, and that is the whole point: by the time somebody is on the download screen the next
    // thing they do is create the account, inside somebody else's sign-in sheet. It is also the
    // FIRST SCREEN OF THE SHEET now (CEO §61) rather than the screen behind a chosen route.
    assert(shown.identity, "the sheet opened past the identity screen");
    assert(!shown.wait, "the download screen is up at the same time as the identity screen");
    assert(
      /no password/i.test(shown.heading),
      "the heading does not state the trap: " + JSON.stringify(shown.heading)
    );
    // The five things §61.1 requires, each one checked for the thing it actually says.
    const required = [
      [/no username and password/i, "that Tailscale has no username and password"],
      [/Google.*Apple.*Microsoft.*GitHub/i, "the four sign-in providers it offers instead"],
      [/identity <?is>? your private network|identity is your private network/i, "that the identity IS the network"],
      [/same/i, "that both devices must use the same one"],
      [/new Google account|only for this/i, "the dedicated-identity way out for somebody who keeps them apart"],
    ];
    for (const [pattern, what] of required) {
      assert(pattern.test(shown.text), "the identity screen never says " + what + ": " + shown.text);
    }
    // And his own loop, named: reinstalling is what he did three times, and it never helps.
    assert(
      /reinstalling tailscale will not help/i.test(shown.text),
      "the screen does not say that reinstalling will not help, which is the thing he did three times"
    );
    // AND THE DOWNLOAD SCREEN IS WHAT IS BEHIND IT — a warning that leads nowhere is a dead end.
    const after = await page.evaluate(() => {
      document.getElementById("phone-identity-ok").click();
      return new Promise((resolve) =>
        setTimeout(
          () =>
            resolve({
              identity: !document.getElementById("phone-identity").hidden,
              heading: document.getElementById("phone-ts-heading").textContent,
            }),
          200
        )
      );
    });
    await page.close();
    assert(!after.identity, "the identity screen does not go away when it has been read");
    assertEqual(
      after.heading,
      "Install Tailscale on this Mac",
      "reading the warning does not lead to the download step"
    );
    return "the identity screen precedes the download step and states all five things, plus the reinstall loop";
  });

  await run.check("12  once the Mac is signed in, the screen names the provider and the login name it used", async () => {
    const page = await openTheFlow("dark", {
      state: "ready",
      name: "mm1.tail9a3b2.ts.net",
      origin: "https://mm1.tail9a3b2.ts.net:8443",
      account: "Google as someone@gmail.com",
    });
    const shown = await page.evaluate(() => ({
      identity: !document.getElementById("phone-identity").hidden,
      ready: !document.getElementById("phone-ts-ready").hidden,
      name: document.getElementById("phone-ts-name").textContent.trim(),
      account: document.getElementById("phone-ts-account").textContent.trim(),
    }));
    await page.close();
    // The warning has done its job the moment detection can name the account, and it does not
    // come back: from here on the screens say WHICH one rather than warning about the choice.
    assert(!shown.identity, "the identity warning is still up after the account is known");
    assert(shown.ready, "the ready screen is not shown for a ready Mac");
    assertEqual(shown.name, "mm1.tail9a3b2.ts.net", "the tailnet name is not shown verbatim");
    assertEqual(
      shown.account,
      "You signed in with Google as someone@gmail.com. Use exactly this on your phone.",
      "the Mac does not name the provider and login name it signed in with"
    );
    return "the ready screen names the machine and " + JSON.stringify(shown.account);
  });

  await run.check("13  the phone step opens with the why, with that account filled in", async () => {
    // Reached the way a person reaches it: the route, the ready screen, then "Set my phone up".
    // A preset that starts mid-pairing would land on the HOME screen, because the route is not
    // remembered across an open — Urban's §1, and `phone.js`'s own `route = null` on open.
    const page = await openTheFlow("dark", {
      state: "ready",
      name: "mm1.tail9a3b2.ts.net",
      origin: "https://mm1.tail9a3b2.ts.net:8443",
      account: "Apple as someone@icloud.com",
    });
    await page.click("#phone-ts-start");
    await page.waitForSelector("#phone-ts-steps:not([hidden])");
    const shown = await page.evaluate(() => {
      const steps = document.getElementById("phone-ts-steps");
      const why = document.getElementById("phone-ts-why");
      const nodes = [...steps.querySelectorAll("p, ol")];
      return {
        why: why.textContent.replace(/\s+/g, " "),
        step2: document.getElementById("phone-ts-step2").textContent.replace(/\s+/g, " "),
        whyIsFirst: nodes.indexOf(why) === 0,
      };
    });
    await page.close();
    assert(shown.whyIsFirst, "the why does not come first on the phone step, so the instruction arrives bare");
    assert(
      /private network/i.test(shown.why),
      "the why does not say the account is the network: " + shown.why
    );
    assert(
      shown.why.includes("Apple as someone@icloud.com"),
      "the why does not name the provider and address the Mac used: " + shown.why
    );
    assert(
      shown.step2.includes("Apple as someone@icloud.com"),
      "step 2 does not name the account either: " + shown.step2
    );
    return "the phone step opens with the why and names " + JSON.stringify("Apple as someone@icloud.com");
  });

  await run.check("14  a phone that is registered and switched off is told to switch it on, not to sign in again", async () => {
    // MEASURED ON THIS MAC, 2026-09-19: the CEO's Android was in the daemon's Peer map TWICE, one
    // stale registration per reinstall, and both were `"Online": false` because Tailscale on the
    // phone was switched off. Read as "your phone is on this network", that state sends somebody
    // who has done everything right back to check their identity.
    const off = await openTheFlow("dark", {
      state: "ready",
      name: "mm1.tail9a3b2.ts.net",
      origin: "https://mm1.tail9a3b2.ts.net:8443",
      account: "Google as someone@gmail.com",
      phone: "Android phone",
      phoneOnline: false,
    });
    const offLine = (await off.textContent("#phone-ts-peer-ready")).replace(/\s+/g, " ").trim();
    await off.close();
    assert(
      /switched off on it/i.test(offLine) && /Turn it on in the Tailscale app/i.test(offLine),
      "a registered, offline phone is not told to switch Tailscale on: " + JSON.stringify(offLine)
    );
    assert(
      !/sign in with/i.test(offLine),
      "an offline phone is told to sign in again, which is the wrong fix: " + JSON.stringify(offLine)
    );

    const on = await openTheFlow("dark", {
      state: "ready",
      name: "mm1.tail9a3b2.ts.net",
      origin: "https://mm1.tail9a3b2.ts.net:8443",
      account: "Google as someone@gmail.com",
      phone: "Android phone",
      phoneOnline: true,
    });
    const onLine = (await on.textContent("#phone-ts-peer-ready")).replace(/\s+/g, " ").trim();
    await on.close();
    assert(
      /is on this network\.?$/i.test(onLine),
      "a live phone is not simply reported as present: " + JSON.stringify(onLine)
    );
    return "offline says " + JSON.stringify(offLine) + "; online says " + JSON.stringify(onLine);
  });

  await run.check("15  with no phone at all, the screen waits and then names the account to use", async () => {
    // The mismatch itself. A phone signed in to a DIFFERENT identity is in a different tailnet and
    // appears nowhere at all, so absence is the evidence — but only after a wait, because before
    // that "not here yet" and "still signing in" are the same thing.
    const page = await openTheFlow("dark", {
      state: "ready",
      name: "mm1.tail9a3b2.ts.net",
      origin: "https://mm1.tail9a3b2.ts.net:8443",
      account: "Google as someone@gmail.com",
    });
    const waiting = (await page.textContent("#phone-ts-peer-ready")).trim();
    assert(
      /waiting for your phone/i.test(waiting),
      "before the grace window the screen already calls it a mistake: " + JSON.stringify(waiting)
    );

    // THE CLOCK IS MOVED, NOT WAITED OUT. `setSystemTime` changes what `Date.now()` answers and
    // triggers no timers, so the sheet's own two-second poll is what redraws — the same path a
    // real minute would take, without spending one.
    await page.clock.setSystemTime(Date.now() + 61000);
    await page.waitForFunction(
      () => !/waiting/i.test(document.getElementById("phone-ts-peer-ready").textContent), null,
      { timeout: 15000 }
    );
    const said = (await page.textContent("#phone-ts-peer-ready")).replace(/\s+/g, " ").trim();
    await page.close();
    assert(
      /not on this network yet/i.test(said),
      "after the wait the screen does not say the phone is absent: " + JSON.stringify(said)
    );
    assert(
      said.includes("Google as someone@gmail.com"),
      "the mismatch line does not name the account to sign in with: " + JSON.stringify(said)
    );
    return "waits with " + JSON.stringify(waiting) + ", then says " + JSON.stringify(said);
  });

  await run.check("16  each link has a control AND its address in writing, and the control asks for that address", async () => {
    // Urban's §2: "a control that opens somewhere the user cannot see first is a control that asks
    // for trust it has not earned". So both, on every screen that names an address — and the
    // written-out form is also the fallback for a Mac where the opener does not work.
    const cases = [
      ["absent", "Get Tailscale", "tailscale.com/download/mac"],
      ["needs-sign-in", "Open Tailscale", "Tailscale.app"],
      ["certificates-off", "Open the Tailscale console", "console.tailscale.com/admin/dns"],
    ];
    const seen = [];
    for (const [state, label, target] of cases) {
      const page = await openTheFlow("dark", { state, name: "mm1.tail9a3b2.ts.net" });
      const shown = await page.evaluate(() => ({
        label: document.getElementById("phone-ts-open").textContent.trim(),
        visible: !document.getElementById("phone-ts-open").hidden,
        url: document.getElementById("phone-ts-url").textContent.trim(),
        urlVisible: !document.getElementById("phone-ts-url").hidden,
      }));
      assertEqual(shown.label, label, "wrong control label for " + state);
      assert(shown.visible, "the control is hidden on " + state);
      await page.click("#phone-ts-open");
      const asked = await page.evaluate(() => window.__RICHOS_OPENED__ || []);
      await page.close();
      assertEqual(asked, [target], "the control on " + state + " asked to open the wrong thing");
      // The sign-in screen names no address, because what it opens is an app the user already has.
      if (target !== "Tailscale.app") {
        assert(shown.urlVisible && shown.url === target, "the address is not written out on " + state);
      }
      seen.push(label + " -> " + target);
    }
    return seen.join("; ");
  });

  // ---- Urban's blocker: a reopen must land on the path the Mac is SERVING ---------------

  for (const theme of ["dark", "light"]) {
    for (const [what, preset] of [
      ["live", { phonePairing: true }],
      ["expired", { phonePairingExpired: true }],
    ]) {
      await run.check(
        `17  reopening the sheet with a ${what} code on the Tailscale path shows the TAILSCALE screen (${theme})`,
        async () => {
          // URBAN'S SIGNOFF BLOCKER, `esc-20260919T041857Z-640acd39`, reproduced twice on the
          // live candidate-.12 window: choose Anywhere, press Set my phone up, close the sheet,
          // reopen it while the code is still live — and the HOME path's screen came back. Two
          // unverified-certificate warnings, a live trust QR at `http://mm1.local:8444/ca` and
          // the sixteen certificate taps, wrapped around the TAILNET pairing URL, on the one
          // path that installs no certificate. The tripwire sentence written to catch exactly
          // that was hidden by the same condition.
          //
          // **THIS PRESET IS THE REOPEN.** The sheet is opened with a code already live (or
          // already expired) and `route === null`, which is precisely the state a reopen is in:
          // `route` is reset on every open by design, and `choosing` requires `!pairing`. On
          // `61568ee1` `onTailscale` came from `route`, so it was `false` here and every
          // Tailscale-only node was hidden and every home-only node shown.
          const page = await openSheet(browser, theme, Object.assign({ phoneTailnet: {
          state: "ready",
          name: "mm1.tail1a2b3c.ts.net",
          origin: "https://mm1.tail1a2b3c.ts.net:8443",
          account: "Google as someone@gmail.com",
        } }, preset));
          await page.waitForSelector("#phone-pairing:not([hidden])");

          const seen = await page.evaluate(() => {
            const vis = (id) => {
              const el = document.getElementById(id);
              return !!el && !el.hidden && el.offsetParent !== null;
            };
            return {
              tailscaleSteps: vis("phone-ts-steps"),
              // The nodes the blocker drew. They are not hidden now, they are not in the
              // markup — so these read `false` for a structural reason rather than a
              // conditional one, which is the strongest form this check can take.
              homeWarnings: !!document.getElementById("phone-home-warnings"),
              trustQrUrl: (document.getElementById("phone-trust-url") || {}).textContent || "",
              lead: (document.getElementById("phone-lead") || {}).textContent || "",
              panel: (document.getElementById("phone-scroll") || {}).innerText || "",
            };
          });

          assert(
            !seen.homeWarnings,
            "THE BLOCKER: the removed path's certificate warnings are back in the markup"
          );
          assert(
            !/8444\/ca/.test(seen.trustQrUrl),
            `THE BLOCKER: the trust URL is on screen: ${JSON.stringify(seen.trustQrUrl)}`
          );
          assert(
            !/sixteen|16 taps/i.test(seen.panel),
            "THE BLOCKER: the sixteen certificate taps are back on a path that installs nothing"
          );
          assert(
            /Tailscale/.test(seen.lead),
            `the opening sentence still describes the path he did not choose: ${JSON.stringify(seen.lead)}`
          );
          if (preset.phonePairing) {
            assert(seen.tailscaleSteps, "the four phone steps are not on the Tailscale screen");
            assert(
              /Nothing has to be installed on your phone/i.test(seen.panel),
              "the tripwire sentence is not on the screen — the one sentence written to catch " +
                "this defect was hidden by the condition that caused it"
            );
          }
          await page.close();
          return `${what} code, ${theme}: tailnet steps ${seen.tailscaleSteps}, home warnings ` +
            `${seen.homeWarnings}, trust URL ${JSON.stringify(seen.trustQrUrl)}`;
        }
      );
    }
  }

  // ---- defect B: the expired card carries no instruction about things that are gone -------

  await run.check("18  an expired code takes its own instructions with it", async () => {
    // Ray's candidate-.12 defect B, frame 19: after the code ran out the card still read "Your
    // phone will show six words. They have to be THESE SIX, in this order ... tap Cancel and
    // tell me" — with no six words under it and no Cancel button anywhere in the sheet. Two
    // things a person is asked to do that are not there. The heading above it and the words
    // below it were already cleared with the code; this paragraph was missed.
    const live = await openSheet(browser, "dark", { phonePairing: true });
    await live.waitForSelector("#phone-pairing:not([hidden])");
    const onLive = await live.evaluate(() => {
      const el = document.getElementById("phone-words-note");
      return { present: !!el && !el.hidden, text: el ? el.textContent.replace(/\s+/g, " ").trim() : "" };
    });
    await live.close();

    const gone = await openSheet(browser, "dark", { phonePairingExpired: true });
    await gone.waitForSelector("#phone-expired:not([hidden])");
    const onExpired = await gone.evaluate(() => {
      const el = document.getElementById("phone-words-note");
      const panel = document.getElementById("phone-scroll");
      return {
        shown: !!el && !el.hidden && el.offsetParent !== null,
        words: (document.getElementById("phone-words") || {}).textContent || "",
        panel: panel ? panel.innerText : "",
      };
    });
    await gone.close();

    assert(onLive.present, "the sentence has to be there while there IS a code to check");
    assert(!onExpired.shown, "THE DEFECT: the six-words instruction survived the code it is about");
    assert(!/these six/i.test(onExpired.panel), "the expired card still says \"these six\"");
    // AND THE CONTROL IT NAMES EXISTS. The sheet has `Show me another code` and `Close`; there
    // has never been a Cancel button in it.
    assert(
      !/tap Cancel/i.test(onLive.text),
      `the sentence names a control this sheet does not have: ${JSON.stringify(onLive.text)}`
    );
    return `live: ${JSON.stringify(onLive.text.slice(0, 60))}…; expired: the sentence is gone and the words are ${JSON.stringify(onExpired.words)}`;
  });

  // ---- G2: the pairing screen has a way back, and it stops serving ------------------------

  await run.check("19  the pairing screen has a way out, and it stops serving", async () => {
    // Urban's G2, seen live and unreachable afterwards: *"Once `Set my phone up` is pressed, the
    // route chooser is unreachable for the life of the app process — `expired` also sets
    // `pairing`, so waiting does not restore it either. I hit this live: after the walk, the
    // route chooser and `This Mac is ready` could not be reached again in dark at all."*
    //
    // BOTH THEMES, because both halves of that sentence are about dark: the screens he could not
    // re-reach were the dark ones, and a fix proved only in light would not answer him.
    const out = [];
    for (const theme of ["dark", "light"]) {
      const page = await openTheFlow(theme, {
        state: "ready",
        name: "mm1.tail9a3b2.ts.net",
        origin: "https://mm1.tail9a3b2.ts.net:8443",
        account: "Google as someone@gmail.com",
      });
      await page.waitForSelector("#phone-ts-ready:not([hidden])");
      await page.click("#phone-ts-start");
      await page.waitForSelector("#phone-pairing:not([hidden])");

      // THE CONTROL IS ON THE SCREEN AND IT IS CLICKABLE, not merely in the markup. When
      // this was written four other copies of the label lived in blocks hidden on this screen,
      // so a check that only looked for the text would pass on a button nobody can press; the
      // other four went with the route chooser (CEO §61) and this is the one that was never
      // about the chooser — it is the one with a socket and a live code behind it.
      const before = await page.evaluate(() => {
        const back = document.getElementById("phone-pairing-back");
        return {
          present: !!back,
          visible: !!back && back.offsetParent !== null,
          label: back ? back.textContent.trim() : "",
          listening: null,
        };
      });
      before.listening = (await page.evaluate(() => window.RichBridge.invoke("phone_status"))).listening;
      assert(before.present, "THE DEFECT: the pairing screen has no way out");
      assert(before.visible, "the way out is in the markup but not on the screen");
      assertEqual(
        before.label,
        "Stop and go back",
        "the control does not say what it does. It read `Pick a different way` while there were " +
          "ways to pick between; §61 leaves one, and a label naming a chooser that does not " +
          "exist is the N1 defect in one word"
      );
      assert(before.listening === true, "the harness never started serving, so stopping proves nothing");

      await page.click("#phone-pairing-back");
      await page.waitForSelector("#phone-ts-ready:not([hidden])");
      const after = await page.evaluate(async () => {
        const status = await window.RichBridge.invoke("phone_status");
        return {
          ready: !document.getElementById("phone-ts-ready").hidden,
          pairing: !document.getElementById("phone-pairing").hidden,
          listening: status.listening,
          pairUrl: status.pairUrl,
        };
      });
      await page.close();

      // WHERE IT LANDS: `This Mac is ready`, which is the screen behind the pairing screen now
      // that there is no chooser in front of it. A Mac that is signed in and certified is still
      // signed in and certified after the window is put down.
      assert(after.ready, "pressing it did not return to `This Mac is ready`");
      assert(!after.pairing, "the pairing screen is still up after backing out of it");
      // **AND IT STOPPED SERVING**, which is the half that is not a button. `pairing` is true
      // while the Mac is listening and unpaired, so a Mac still answering would redraw the
      // pairing screen on the next poll and this control would read as one that does nothing.
      assert(after.listening === false, "THE DEFECT'S SECOND HALF: the Mac is still serving after `Stop and go back`");
      assert(after.pairUrl === null, "the code he backed out of is still live");
      out.push(theme + ": listening " + before.listening + " -> " + after.listening + ", back on `This Mac is ready`");
    }
    return out.join("; ");
  });

  // ---- G3/G4/G5: what the screen puts first, where it puts him back, and what it stopped -----

  await run.check("20  the code is ABOVE the phone steps, and no heading numbers a sequence that is not there", async () => {
    // Urban's G3, measured on his own frames 04 and 05: at 1024x700 — the app's own minimum and
    // the size it restores itself to — the pairing screen is about three viewport heights tall,
    // and the code was about 1.5 screens down. *"The four phone steps are preparation for a
    // person who has not started; the code is what a person who is standing there with their
    // phone needs. Order the screen for the second person."*
    //
    // THIS CHECK USED TO HAVE A SECOND HALF, and it is worth saying what it was. The other route
    // numbered its three headings 1, 2, 3 and the numbers were a real sequence — the certificate
    // at step 1 was what made step 2's address open at all — so a fix that moved the code on
    // BOTH routes would have put step 2 above step 1, and reading order was asserted in opposite
    // directions on the two. CEO §61 removed that route, and with it the only sequence on this
    // screen; what is left is Urban's N4, which is that a heading must not say "Then" or "2."
    // over a screen where nothing comes before it.
    //
    // AND IT IS THE MARKUP'S ORDER NOW, not a node the render moves. That is asserted here by
    // measuring the DOM with no render having had a chance to reorder anything: the page is
    // opened on a live window, so if the order were still being produced at render time this
    // check would pass and the `hidden` case below would not.
    const order = async (page, a, b) =>
      page.evaluate((s) => {
        const first = document.querySelector(s.a);
        const second = document.querySelector(s.b);
        if (!first || !second) return "missing";
        // 4 === DOCUMENT_POSITION_FOLLOWING: `b` comes after `a` in the document.
        return first.compareDocumentPosition(second) & 4 ? "a-then-b" : "b-then-a";
      }, { a, b });

    const ts = await openTheFlow("dark", READY_TAILNET);
    await ts.click("#phone-ts-start");
    await ts.waitForSelector("#phone-pairing:not([hidden])");
    const live = await order(ts, "#phone-code-block", "#phone-ts-steps");
    const headings = await ts.evaluate(() => ({
      code: document.getElementById("phone-code-title").textContent.trim(),
      words: document.getElementById("phone-words-title").textContent.trim(),
      codeVisible: !document.getElementById("phone-code-block").hidden,
    }));
    await ts.close();

    // AND THE SAME ORDER ON AN EXPIRED WINDOW, where the code block is hidden rather than
    // absent. The mover's idempotence note was about exactly this case going wrong on a poll.
    const gone = await openSheet(browser, "dark", {
      phonePairingExpired: true,
      phoneTailnet: READY_TAILNET,
    });
    await gone.waitForSelector("#phone-pairing:not([hidden])");
    const expiredOrder = await order(gone, "#phone-code-block", "#phone-ts-steps");
    await gone.close();

    assert(headings.codeVisible, "the code block is hidden on a screen with a live code");
    assertEqual(live, "a-then-b", "THE DEFECT: the four phone steps still come before the code");
    assertEqual(expiredOrder, "a-then-b", "the order moved when the code expired");
    // URBAN'S N4: *"Then"* pointed backwards at a step G3 had moved below it. There is nothing
    // above either heading now, so neither may claim there is.
    assert(
      !/^\d\./.test(headings.code) && !/\bThen\b/.test(headings.code),
      "the code heading numbers or refers back to a step that is not above it: " + JSON.stringify(headings.code)
    );
    assert(
      !/^\d\./.test(headings.words),
      "the six-words heading numbers a sequence that is not there: " + JSON.stringify(headings.words)
    );
    return (
      "code then steps, live and expired alike, from the markup's own order; headings " +
      JSON.stringify([headings.code, headings.words]) + " with no numbering and no backward reference"
    );
  });

  await run.check("21  `Show me another code` puts the code back on screen, not the top of the sheet", async () => {
    // Urban's G4, frames 08 and 09: the button issued the code AND returned the panel to the
    // top, *"so the user must scroll the whole way down again to reach the thing they just
    // asked for"*. Walked on the HOME route, where the code sits in the middle of the screen
    // and so the two outcomes — "scrolled to the top" and "the code is in view" — are different
    // answers. On the Tailscale route G3 already puts the code at the top and they coincide.
    const page = await openSheet(browser, "dark", { phonePairing: true });
    await page.waitForSelector("#phone-pairing:not([hidden])");
    // Where he is when he presses it: at the bottom, on the button.
    await page.evaluate(() => {
      const panel = document.getElementById("phone-scroll");
      panel.scrollTop = panel.scrollHeight;
    });
    await page.click("#phone-refresh");
    // The code coming into view is the fact waited for (hang guard); it used to be a 250ms
    // sleep (2026-09-29, audit R10). The assertion below still names where the code ended up.
    await page
      .waitForFunction(
        () => {
        const panel = document.getElementById("phone-scroll");
        const code = document.getElementById("phone-code-block");
        if (!panel || !code) return false;
        const p = panel.getBoundingClientRect();
        const c = code.getBoundingClientRect();
        return c.top - p.top >= 0 && c.top - p.top < p.height - 40;
      },
        null,
        { timeout: 10000 }
      )
      .catch(() => {});
    const where = await page.evaluate(() => {
      const panel = document.getElementById("phone-scroll");
      const code = document.getElementById("phone-code-block");
      const p = panel.getBoundingClientRect();
      const c = code.getBoundingClientRect();
      return {
        scrollTop: Math.round(panel.scrollTop),
        scrollable: Math.round(panel.scrollHeight - panel.clientHeight),
        codeTopInPanel: Math.round(c.top - p.top),
        panelHeight: Math.round(p.height),
      };
    });
    await page.close();
    assert(where.scrollable > 40, "the panel does not scroll in this window, so this check proves nothing");
    // THE CODE IS IN VIEW. Stated as geometry rather than as a scroll number, because the
    // requirement is about what he can see and not about where the scrollbar ended up.
    assert(
      where.codeTopInPanel >= 0 && where.codeTopInPanel < where.panelHeight - 40,
      "THE DEFECT: after `Show me another code` the code is at " + where.codeTopInPanel +
        "px in a " + where.panelHeight + "px panel — off the screen he is looking at"
    );
    return "panel scrolls " + where.scrollable + "px; after the fresh code the block sits " +
      where.codeTopInPanel + "px into a " + where.panelHeight + "px panel (scrollTop " + where.scrollTop + ")";
  });

  await run.check("22  no raw socket dump on the pairing screen, on either route", async () => {
    // Urban's G5, his frame 06: eight address:port pairs over four lines, drawn whenever a code
    // was live, on both routes, for every user, and not behind the technical view. *"It tells
    // the reader nothing they can act on, and it is the one thing on this flow that looks like a
    // debug log that shipped."* He was given a choice of Techy Mode or nowhere and said nowhere.
    const found = [];
    const home = await openSheet(browser, "dark", { phonePairing: true });
    await home.waitForSelector("#phone-pairing:not([hidden])");
    found.push(await home.evaluate(() => ({
      route: "home",
      node: !!document.getElementById("phone-bound"),
      text: document.getElementById("phone-scroll").innerText,
    })));
    await home.close();

    const ts = await openTheFlow("dark", {
      state: "ready", name: "mm1.tail9a3b2.ts.net",
      origin: "https://mm1.tail9a3b2.ts.net:8443", account: "Google as someone@example.com",
    });
    await ts.click("#phone-ts-start");
    await ts.waitForSelector("#phone-pairing:not([hidden])");
    found.push(await ts.evaluate(() => ({
      route: "tailnet",
      node: !!document.getElementById("phone-bound"),
      text: document.getElementById("phone-scroll").innerText,
    })));
    await ts.close();

    for (const f of found) {
      assert(!f.node, "THE DEFECT: the bound-address node is still in the sheet on the " + f.route + " route");
      assert(!/answering on/i.test(f.text), "the sheet still says what it is answering on (" + f.route + ")");
      // The pattern the dump is made of, and the one the pairing URL is NOT: an address with a
      // port. `https://mm1.local:8443/#pair=…` is a name and a port and does not match.
      const dump = f.text.match(/\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}:\d+/g);
      assertEqual(dump, null, "an address:port pair is still on the " + f.route + " screen");
    }
    return "both routes: no #phone-bound, no \"answering on\", no address:port pair in the rendered panel";
  });

  // ---- G6: the border that is the only affordance on the paired card ----------------------

  await run.check("23  `.desk-btn`'s border clears 3:1 in BOTH themes — measured from the pixels", async () => {
    // Urban's G6, and the one contrast failure in his audit: *"On the paired card that border is
    // the only thing that makes a button a button. Every line on that card is `--ink-soft` at the
    // same size and weight — labels and paragraphs alike ... So at 1.24:1 in dark, the card's two
    // controls have no visible affordance at all."*
    //
    // MEASURED THE WAY HE MEASURED IT: a crop of the rendered pixels across the border, ground =
    // the modal value on the crop's skirt, ink = the value furthest from it in luminance, and the
    // ratio between them. `lib/contrast.js`'s own `measureIndicatorCrop`, which is the function
    // the indicator subset of the walk uses — not a second implementation, and not a token table.
    const { measureIndicatorCrop } = require("./lib/contrast");
    const out = [];
    for (const theme of ["dark", "light"]) {
      const page = await openSheet(browser, theme, { phonePaired: true });
      await page.waitForFunction(() => {
        const n = document.getElementById("phone-device-name");
        return n && n.textContent.trim().length > 0;
      });
      // Every finite transition on the sheet has ENDED, rather than 300ms having passed
      // (2026-09-29, audit R10): `awaitSettled` waits on `Animation.finished` with a hang guard.
      await awaitSettled(page);
      for (const sel of ["#phone-forget", "#phone-close"]) {
        const box = await page.evaluate((s) => {
          const el = document.querySelector(s);
          el.scrollIntoView({ block: "center" });
          const r = el.getBoundingClientRect();
          return { x: r.x, y: r.y, w: r.width, h: r.height };
        }, sel);
        // Two of the page's own frames for the scroll to be painted, not 60ms (audit R10).
        await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
        // A 6px strip across the LEFT border, taken from the vertical middle so no glyph of the
        // label is inside it: columns 0/1 are the panel outside the button and 4/5 the panel
        // showing through it (`background: none`), so the skirt resolves to the panel and the
        // two border columns are the only other thing in the crop.
        const png = await page.screenshot({
          clip: {
            x: Math.round(box.x - 3),
            y: Math.round(box.y + box.h * 0.35),
            width: 6,
            height: Math.max(6, Math.round(box.h * 0.3)),
          },
        });
        const m = measureIndicatorCrop(png, 3);
        assert(!m.unresolvable, sel + " in " + theme + ": " + m.unresolvable);
        assert(
          m.ratio >= 3,
          "THE DEFECT: " + sel + "'s border is " + m.ratio + ":1 in " + theme + " (" + m.ink +
            " on " + m.ground + ") against a 3:1 floor for a non-text indicator — and on this card " +
            "it is the only thing that makes a button a button"
        );
        out.push(theme + " " + sel + " " + m.ratio + ":1 (" + m.ink + " on " + m.ground + ")");
      }
      await page.close();
    }
    return out.join("; ");
  });

  // ---- G8: the paired card has a top, and its one errand is not housekeeping ---------------

  await run.check("24  the paired card is headed by the phone's name, and the push line is the strongest line on it", async () => {
    // Urban's G8: *"Paired card has no heading and one text color for all five paragraphs — the
    // actionable line reads like housekeeping."* Ray's frames 21 and 22.
    const out = [];
    for (const theme of ["dark", "light"]) {
      const page = await openSheet(browser, theme, { phonePaired: true, phonePlatform: "android" });
      await page.waitForFunction(() => {
        const n = document.getElementById("phone-device-name");
        return n && n.textContent.trim().length > 0;
      });
      const card = await page.evaluate(() => {
        const heading = document.getElementById("phone-device-name");
        const push = document.getElementById("phone-push-state");
        const note = document.getElementById("phone-forget-note");
        const styleOf = (el) => {
          const s = getComputedStyle(el);
          return { color: s.color, size: s.fontSize, weight: s.fontWeight, tag: el.tagName };
        };
        return {
          heading: Object.assign(styleOf(heading), { text: heading.textContent.trim(), cls: heading.className }),
          push: Object.assign(styleOf(push), { text: push.textContent.trim() }),
          note: styleOf(note),
          // Round 49 puts the status chip beside the name, in one top row that holds the heading.
          first: document.getElementById("phone-paired").firstElementChild.contains(heading) ? "phone-device-name" : document.getElementById("phone-paired").firstElementChild.id,
        };
      });
      await page.close();
      // THE HEADING: the device name, as a heading, and it is the first thing on the card.
      assertEqual(card.heading.tag, "H3", "the device name is not a heading — Urban's G8");
      assert(/phone-step-title/.test(card.heading.cls), "the heading is not a `.phone-step-title`: " + card.heading.cls);
      assertEqual(card.first, "phone-device-name", "something comes before the heading on the paired card");
      assert(card.heading.text.length > 0, "the heading is empty");
      // THE PUSH LINE: --ink rather than --ink-soft, so the one line with an errand in it is no
      // longer the same ink as the sentence about removing a certificate.
      assert(
        card.push.color !== card.note.color,
        "THE DEFECT: the push line is the same ink as the paragraphs around it (" + card.push.color + ")"
      );
      out.push(theme + ": heading " + JSON.stringify(card.heading.text) + ", push " + card.push.color +
        " vs prose " + card.note.color);
    }
    return out.join("; ");
  });

  // ---- G13: a countdown that never overpromises --------------------------------------------

  await run.check("25  the countdown never claims more time than the code has", async () => {
    // Urban's G13, filmed at 30-second intervals: *"Countdown says '2 more minutes' at 61 s
    // remaining (`Math.ceil`), then jumps to '59 more seconds'."*
    //
    // Asserted as the PROPERTY and not as one string: at every second measured, the number of
    // whole minutes the label claims must be time the code actually has. 61 is the value the
    // defect lives at; 120, 60 and 59 are the values on either side of it, which keep the fix
    // from being a hard-coded answer to one number.
    // AND THE CHECK PROVES IT SAW THE SECOND IT ASKED FOR. The first version of this did not:
    // the harness started the window's clock in the past and the clock kept running, so by the
    // time anything read the screen it was at 60 or below, where `ceil` and `floor` agree. Run
    // against `2c95c27d` — which still had `Math.ceil` — it printed `61s -> "1 more minute."`,
    // the fixed tree's answer from the broken tree. So the status is read at the same moment as
    // the label and asserted to still BE the number requested; a harness that drifts now fails
    // here instead of passing everywhere.
    const seen = [];
    for (const secondsLeft of [300, 120, 61, 60, 59, 1]) {
      const page = await openSheet(browser, "dark", { phonePairing: true, phonePairingSecondsLeft: secondsLeft });
      await page.waitForSelector("#phone-pairing:not([hidden])");
      const reading = await page.evaluate(async () => ({
        label: document.getElementById("phone-countdown").textContent.trim(),
        reported: (await window.RichBridge.invoke("phone_status")).pairingSecondsLeft,
      }));
      const label = reading.label;
      await page.close();
      assertEqual(
        reading.reported,
        secondsLeft,
        "the harness did not hold the window at " + secondsLeft + "s, so this reading is of some " +
          "other second and the check proves nothing"
      );
      seen.push(secondsLeft + "s -> " + JSON.stringify(label));
      const minutes = label.match(/(\d+) more minutes?/);
      if (minutes) {
        const claimed = Number(minutes[1]) * 60;
        assert(
          claimed <= secondsLeft,
          "THE DEFECT: with " + secondsLeft + "s left the screen says " + JSON.stringify(label) +
            " — " + (claimed - secondsLeft) + "s more than the code has"
        );
      } else {
        const secs = label.match(/(\d+) more seconds?/);
        assert(secs, "the countdown said neither minutes nor seconds: " + JSON.stringify(label));
        assert(Number(secs[1]) <= secondsLeft, "the seconds label overpromises: " + JSON.stringify(label));
      }
    }
    return seen.join("; ");
  });

  // ---- G14: the screen that names the account first stops whispering it ---------------------

  await run.check("26  on `This Mac is ready` the account is bold and sits directly under the heading", async () => {
    // Urban's G14: *"That asymmetry is backwards: the screen that names the account first is the
    // one that whispers it ... it is the thing that decides whether this works, and the machine
    // name is not."* `phone.js` used `textContent` here while the two screens below it use
    // `innerHTML` with `<strong>` — *"one of the three is wrong and it is the first one."*
    const page = await openTheFlow("dark", {
      state: "ready", name: "mm1.tail9a3b2.ts.net",
      origin: "https://mm1.tail9a3b2.ts.net:8443", account: "Google as someone@example.com",
    });
    await page.waitForSelector("#phone-ts-ready:not([hidden])");
    const screen = await page.evaluate(() => {
      const box = document.getElementById("phone-ts-ready");
      const account = document.getElementById("phone-ts-account");
      const strong = account.querySelector("strong");
      return {
        order: [...box.children].map((n) => n.id || n.tagName.toLowerCase()),
        strong: strong ? strong.textContent.trim() : null,
        text: account.textContent.replace(/\s+/g, " ").trim(),
      };
    });
    await page.close();
    // HEADING, ACCOUNT, NAME — in that order, and the sentence after the name still points at it.
    assertEqual(
      screen.order.slice(0, 3),
      ["h3", "phone-ts-account", "phone-ts-name"],
      "THE DEFECT: the account sentence is not directly under the heading, above the machine name"
    );
    assertEqual(screen.strong, "Google as someone@example.com", "the account is not given weight on the screen that names it first");
    assert(/Use exactly this on your phone/.test(screen.text), "the sentence lost its instruction: " + screen.text);
    return "order " + JSON.stringify(screen.order.slice(0, 3)) + ", account in <strong>: " + JSON.stringify(screen.strong);
  });

  // ---- N1: the chooser has two doors, and both of them now lead where they say ------------

  await run.check(
    "27  the Mac serves the one path, and the account §61.1 exists for is named on the screen",
    async () => {
      // **WHAT THIS CHECK WAS, AND WHY IT IS THIS NOW.**
      //
      // It was Urban's N1 blocker (signoff 2026-09-19 10.40, frames 10 -> 11 -> 12): he pressed
      // `At home only` — *"there is no account to make"* — then `Set my phone up`, and got the
      // Tailscale screen with a `…ts.net` pairing URL and his own account in step 2. *"A product
      // that asks a question and ignores the answer has spent the trust the rest of this flow
      // earned."* The fix was to carry the answer across the bridge, and this check walked both
      // doors.
      //
      // CEO §61, the next day, removed the question: *"any mobile app or PWA is utterly useless
      // within the home network … The Tailscale setup is where we start now."* A product that
      // asks no question cannot ignore an answer. So the half of N1 that survives is the half
      // that was always about the product rather than about the chooser — what the Mac actually
      // serves, and whether the screen names the account §61.1 exists for.
      //
      // **THE MAC IN THIS CHECK IS HIS MAC**: `state: "ready"`, an account, a certificate that
      // would issue. It is the only Mac this ships against.
      //
      // **BOTH THEMES**, because his acceptance says both.
      const READY = READY_TAILNET;
      // What the user can actually READ: `innerText` and not `textContent`, because a hidden
      // node is still in the DOM. Being *in the DOM* is not the defect; being *on the screen* is.
      const readScreen = () =>
        (async () => {
          const status = await window.RichBridge.invoke("phone_status");
          const shown = (id) => {
            const node = document.getElementById(id);
            return !!node && node.offsetParent !== null;
          };
          return {
            servingVia: status.servingVia,
            pairUrl: (document.getElementById("phone-pair-url").textContent || "").trim(),
            tsSteps: shown("phone-ts-steps"),
            visibleText: document
              .getElementById("phone-sheet")
              .innerText.replace(/\s+/g, " ")
              .trim(),
          };
        })();

      const out = [];
      for (const theme of ["dark", "light"]) {
        const tail = await openTheFlow(theme, READY);
        await tail.waitForSelector("#phone-ts-ready:not([hidden])");
        await tail.click("#phone-ts-start");
        await tail.waitForSelector("#phone-pairing:not([hidden])");
        const served = await tail.evaluate(readScreen);
        await tail.close();

        assertEqual(served.servingVia, "tailnet", "the Mac is not serving the tailnet (" + theme + ")");
        assert(
          /^https:\/\/mm1\.tail9a3b2\.ts\.net:8443\/#pair=/.test(served.pairUrl),
          "the code no longer goes out under the tailnet name: " + JSON.stringify(served.pairUrl)
        );
        assert(served.tsSteps, "the four Tailscale steps are gone from the screen that is the Tailscale path");
        // **AND THE ACCOUNT IS NAMED**, which is the whole of §61.1: the one thing the user does
        // not know is which identity this Mac used, and "use the same account" is advice nobody
        // can follow without it.
        assert(
          served.visibleText.includes("someone@gmail.com"),
          "the screen stopped naming the account, which is the whole of §61.1 (" + theme + ")"
        );
        // AND NOTHING OF THE REMOVED PATH IS ON THE SCREEN. Not hidden — the nodes are gone —
        // but read back from what a person sees, because that is the claim.
        for (const forbidden of ["mm1.local", ":8444", "Unverified", "Certificate Trust", "at home"]) {
          assert(
            !served.visibleText.toLowerCase().includes(forbidden.toLowerCase()),
            "the removed path's " + JSON.stringify(forbidden) + " is on screen (" + theme + ")"
          );
        }

        out.push(
          theme + ": servingVia " + served.servingVia + ", " + served.pairUrl.split("/#")[0] +
            ", four phone steps, account named"
        );
      }
      return out.join("; ");
    }
  );

  await run.check(
    "28  reopening the sheet with a live code lands on the code, not at the bottom of the screen",
    async () => {
      // URBAN'S N2 (HIGH), his frame 05 versus 06. G12's "reset the scroll on open" was applied
      // to the SETTINGS PANEL and not to the phone sheet, so the reopen that G1 was about — close
      // the sheet mid-pairing, go and get your phone, come back — lands at the BOTTOM of a
      // two-viewport screen, past the live code the user came back for. *"This is G3 and G4's own
      // principle failing on the third door into the same screen."*
      //
      // **THE STATE IS PRODUCED THE WAY A PERSON PRODUCES IT**: scroll down, close, reopen. It is
      // not injected by setting `scrollTop` after the open, which would test the assignment
      // rather than the behavior.
      // `expectCode: false` is a reopen with NO live code (the expired state): `#phone-code-block`
      // is deliberately `hidden` there (`phone.js` `codeBlock.hidden = !live || ...`), its
      // rectangle is zero, and the geometry below can never become true. Waiting for it spent the
      // whole 10 s guard on every normal run and swallowed the timeout (hunt part 2, finding 43).
      const reopen = async (page, { expectCode = true } = {}) => {
        await page.click("#phone-close");
        // `state: "hidden"` rather than a `[hidden]` selector: the default wait is for a VISIBLE
        // match, which a hidden sheet can never be, and the check would time out at 30 s green-
        // looking-red rather than saying what it saw.
        await page.waitForSelector("#phone-sheet", { state: "hidden" });
        if (!(await page.isVisible("#set-phone-open"))) await page.click("#set-btn");
        await page.waitForSelector("#set-phone-open");
        await page.click("#set-phone-open");
        await page.waitForSelector("#phone-sheet:not([hidden])");
        // The open is `async` (it awaits `phone_status`), so the screen it draws — and the scroll
        // it sets on the back of that draw — is one turn later than the click. That scroll landing
        // the code in view is the fact waited for (hang guard), not 250ms (audit R10); on a build
        // that never scrolls, this runs out and the assertion below names where the code is.
        if (!expectCode) {
          // The fact to wait for here is the screen that IS drawn: the expired card.
          await page.waitForSelector("#phone-expired:not([hidden])");
          return;
        }
        await page
          .waitForFunction(
            () => {
            const panel = document.getElementById("phone-scroll");
            const code = document.getElementById("phone-code-block");
            if (!panel || !code) return false;
            const p = panel.getBoundingClientRect();
            const c = code.getBoundingClientRect();
            return c.top - p.top >= 0 && c.top - p.top < p.height - 40;
          },
            null,
            { timeout: 10000 }
          )
          .catch(() => {});
      };
      const where = (page) =>
        page.evaluate(() => {
          const panel = document.getElementById("phone-scroll");
          const code = document.getElementById("phone-code-block");
          const p = panel.getBoundingClientRect();
          const c = code.getBoundingClientRect();
          return {
            scrollTop: Math.round(panel.scrollTop),
            scrollable: Math.round(panel.scrollHeight - panel.clientHeight),
            codeHidden: code.hidden,
            codeTopInPanel: Math.round(c.top - p.top),
            panelHeight: Math.round(p.height),
          };
        });

      // **THE SCROLL SURVIVES THE HIDE, MEASURED BEFORE THIS CHECK WAS WRITTEN.** `sheet.hidden`
      // makes the panel `display: none`, and it would have been reasonable to assume WebKit
      // forgets a scroll offset it cannot render. It does not: probed on this build, the panel
      // came back at **708 of 708** at 1400x950 and at **928 of 928** at 1024x700, both exactly
      // where they were left. So the defect is reproducible here and this check can go red.
      //
      // **AND IT IS WALKED ON THE TAILSCALE ROUTE, WHICH IS URBAN'S OWN.** On the home route the
      // code block sits low enough that the bottom of the scroll still has it in view (codeTop
      // 344 of an 812px panel, measured) — so a check written there would have passed on the
      // broken build, for a reason that is about G3's one-node-two-positions and nothing to do
      // with N2. Both routes are walked; only the second could have caught this.
      //
      // The assertion is the same on both, and it is check 21's: the code is IN VIEW. It is not
      // "at the top", because on the home route **the top is unreachable** — the code block sits
      // inside the last viewport of that document, so the maximum scroll and `scrollToCode()`
      // land in the same place, 344px into an 812px panel. That is the same number check 21
      // reports for `Show me another code`, and it is why the home half of this check passes on
      // the broken build: it is a non-regression, and the Tailscale half is the proof.
      const out = [];
      for (const route of ["home", "tailnet"]) {
        const page =
          route === "tailnet"
            ? await openTheFlow("dark", {
                state: "ready",
                name: "mm1.tail9a3b2.ts.net",
                origin: "https://mm1.tail9a3b2.ts.net:8443",
                account: "Google as someone@gmail.com",
              })
            : await openSheet(browser, "dark", { phonePairing: true });
        if (route === "tailnet") {
          await page.waitForSelector("#phone-ts-ready:not([hidden])");
          await page.click("#phone-ts-start");
        }
        await page.waitForSelector("#phone-pairing:not([hidden])");
        await page.evaluate(() => {
          const panel = document.getElementById("phone-scroll");
          panel.scrollTop = panel.scrollHeight;
        });
        const before = await where(page);
        await reopen(page);
        const after = await where(page);
        await page.close();

        assert(before.scrollable > 40, route + ": the panel does not scroll in this window, so this check proves nothing");
        assert(before.scrollTop > 40, route + ": the walk never reached the bottom, so the reopen has nothing to undo");
        assert(!after.codeHidden, route + ": the code went away across the reopen, which is a different defect");
        assert(
          after.codeTopInPanel >= 0 && after.codeTopInPanel < after.panelHeight - 40,
          "THE DEFECT (" + route + "): after reopening, the live code sits at " +
            after.codeTopInPanel + "px in a " + after.panelHeight + "px panel — he came back for " +
            "the code and the sheet gave him the place he left"
        );
        out.push(
          route + ": left at " + before.scrollTop + "px of " + before.scrollable +
            ", reopened with the code " + after.codeTopInPanel + "px from the top"
        );
      }

      // ---- and with NO live code the other branch runs: the top of the screen, never the middle
      // of a screen he has never seen. The expired screen is one viewport by design (Urban's G7),
      // so this half is a guard on the branch rather than a geometry measurement, and it says so.
      const expired = await openSheet(browser, "dark", { phonePairingExpired: true });
      await expired.waitForSelector("#phone-expired:not([hidden])");
      await expired.evaluate(() => {
        const panel = document.getElementById("phone-scroll");
        panel.scrollTop = panel.scrollHeight;
      });
      await reopen(expired, { expectCode: false });
      const rest = await where(expired);
      await expired.close();
      assertEqual(rest.scrollTop, 0, "a reopen with no live code did not land at the top of the sheet");
      out.push(
        "no live code: reopened at scrollTop 0 (that screen scrolls " + rest.scrollable +
          "px in this window)"
      );
      return out.join("; ");
    }
  );

  await run.check(
    "29  one way out, on the one screen with something to put down, and no copy that does nothing",
    async () => {
      // **URBAN'S N3, AND WHAT §61 DID TO IT.**
      //
      // N3 (MEDIUM), his frame 10: *"`Pick a different way` is on Screen 0, on Screens 2/3/7, on
      // `This Mac is ready` and — since G2 — on the pairing screen. It is absent from exactly
      // one: the screen you land on by answering the question."* The fix was a fifth copy, on
      // the screen the answer led to.
      //
      // CEO §61 removed the question and the screen the answer led to, and with them the reason
      // four of those five copies existed: every one of them returned to the chooser, and a
      // chooser that does not exist is a destination that does not exist. A button that returns
      // to the screen you are standing on is a control that does nothing, which is the defect
      // N3 was about pointing the other way.
      //
      // So what N3's finding becomes is this: the way out is on the ONE screen that has
      // something to put down — a live code and an open socket — and nowhere else, and Close is
      // on every screen as it always was. Check 19 walks what that control does; this one walks
      // how many of it there are, and what the other screens offer instead.
      const screens = [
        { what: "identity", tailnet: { state: "absent" }, id: "phone-identity" },
        {
          what: "waiting",
          tailnet: { state: "certificates-off", account: "Google as someone@gmail.com" },
          id: "phone-ts-wait",
        },
        { what: "ready", tailnet: READY_TAILNET, id: "phone-ts-ready" },
      ];
      const seen = [];
      for (const screen of screens) {
        const page = await openSheet(browser, "dark", { phoneTailnet: screen.tailnet });
        await page.waitForSelector("#" + screen.id + ":not([hidden])");
        const state = await page.evaluate(async () => {
          const status = await window.RichBridge.invoke("phone_status");
          const visible = [...document.querySelectorAll("#phone-sheet button")].filter(
            (b) => b.offsetParent !== null
          );
          return {
            labels: visible.map((b) => b.textContent.trim()),
            // EVERY copy in the markup, hidden ones included — a control that comes back on a
            // screen this loop does not open would still be counted here.
            backCopies: [...document.querySelectorAll("#phone-sheet button")]
              .filter((b) => /go back|different way/i.test(b.textContent))
              .map((b) => b.id),
            listening: status.listening,
          };
        });
        await page.close();
        assertEqual(
          state.backCopies,
          ["phone-pairing-back"],
          screen.what + ": the way out exists on more than the one screen that has something to " +
            "put down, so at least one copy of it returns to where it was pressed"
        );
        assert(
          state.labels.includes("Close"),
          screen.what + ": Close is not on the screen, and it is the way out of every one of them"
        );
        assertEqual(
          state.listening,
          false,
          screen.what + ": something is serving on a screen that has started nothing"
        );
        seen.push(screen.what + " -> " + JSON.stringify(state.labels));
      }
      return "one back control in the markup (phone-pairing-back), Close on all of: " + seen.join("; ");
    }
  );

  // CHECK 30 WAS URBAN'S N4 and it is now inside check 20, where the headings are.
  //
  // N4 (LOW), frames 03 and 15: G3 moved the code heading to the top of the screen and *"Then"*
  // stayed, pointing backwards at a step that was now underneath it. Its second half — *"and the
  // home route keeps its own 2. Then, which is the half that makes this a fix rather than a
  // find-and-replace"* — was about a route CEO §61 removed, and a check whose two halves are a
  // comparison cannot keep only one half and stay a comparison. So the surviving assertion moved
  // to the check that measures reading order on this screen: no heading may number or refer back
  // to a step that is not above it, which is N4's finding without its counterexample.

  await run.check(
    "31  a reopened sheet asks for a fresh code, and the Mac stays on the channel it has up",
    async () => {
      // WHAT THIS CHECK WAS FOR, AND WHY IT IS STILL HERE. `phone_begin_pairing` briefly carried
      // the user's chosen route (Urban's N1) while `open()` forgot that choice on every open
      // (Urban §1), so `Show me another code` after a reopen sent `route: null` — and the risk
      // was that a returning user pressed the one button on the screen and got a code on the
      // other path. CEO §61 removes the route and the other path, so the argument is gone from
      // both sides of the bridge.
      //
      // The property it pinned is not gone: a channel that is already up must not be re-planned
      // by a second call. `PhoneRuntime::start` returns early for a running channel, and
      // `servingVia` keeps reporting the path it came up on. A build that re-planned would tear
      // a live socket down under a user who pressed a button labeled "another code".
      const page = await openTheFlow("dark", READY_TAILNET);
      await page.waitForSelector("#phone-ts-ready:not([hidden])");
      await page.click("#phone-ts-start");
      await page.waitForSelector("#phone-pairing:not([hidden])");
      const started = await page.evaluate(() => window.RichBridge.invoke("phone_status"));

      await page.click("#phone-close");
      await page.waitForSelector("#phone-sheet", { state: "hidden" });
      if (!(await page.isVisible("#set-phone-open"))) await page.click("#set-btn");
      await page.click("#set-phone-open");
      await page.waitForSelector("#phone-sheet:not([hidden])");
      await page.waitForSelector("#phone-pairing:not([hidden])");
      // A SECOND CALL INTO A RUNNING CHANNEL, which is what makes the press below the
      // interesting one rather than a repeat of check 27.
      const urlBefore = await page.evaluate(() => (document.getElementById("phone-pair-url").textContent || "").trim());
      await page.click("#phone-refresh");
      // The fresh code on screen (a different pairing URL) is the fact waited for, with a hang
      // guard; this was a 250ms sleep (2026-09-29, audit R10).
      await page
        .waitForFunction(
          (u) => {
            const t = (document.getElementById("phone-pair-url").textContent || "").trim();
            return t.length > 0 && t !== u;
          },
          urlBefore,
          { timeout: 10000 }
        )
        .catch(() => {});

      const after = await page.evaluate(async () => {
        const status = await window.RichBridge.invoke("phone_status");
        const shown = (id) => {
          const node = document.getElementById(id);
          return !!node && node.offsetParent !== null;
        };
        return {
          servingVia: status.servingVia,
          pairUrl: (document.getElementById("phone-pair-url").textContent || "").trim(),
          tsSteps: shown("phone-ts-steps"),
          heading: document.getElementById("phone-code-title").textContent.trim(),
        };
      });
      await page.close();

      assertEqual(started.servingVia, "tailnet", "the harness never served over the tailnet, so this proves nothing");
      assertEqual(
        after.servingVia,
        "tailnet",
        "THE DEFECT: a fresh code asked for after a reopen moved the Mac to the " + after.servingVia + " path"
      );
      assert(
        /^https:\/\/mm1\.tail9a3b2\.ts\.net:8443\/#pair=/.test(after.pairUrl),
        "the fresh code went out under a different origin: " + JSON.stringify(after.pairUrl)
      );
      assert(after.tsSteps, "the four phone steps went away on a reopen");
      assertEqual(after.heading, "Point your phone's camera at this", "the code heading changed on a reopen");
      return "reopened, pressed `Show me another code`: servingVia " +
        started.servingVia + " -> " + after.servingVia + ", " + after.pairUrl.split("/#")[0];
    }
  );

  await run.check("10  nothing on the sheet threw, in either theme", async () => {
    const errors = [];
    let combinations = 0;
    for (const theme of ["dark", "light"]) {
      for (const preset of [
        { phonePairing: true, phoneTailnet: READY_TAILNET },
        { phonePaired: true, phoneTailnet: READY_TAILNET },
        // And the three how-to screens, which are where a first-time user actually stands.
        { phoneTailnet: { state: "absent" } },
        { phoneTailnet: { state: "certificates-off", account: "Google as someone@gmail.com" } },
        { phoneTailnet: READY_TAILNET },
      ]) {
        const page = await openSheet(browser, theme, preset);
        await page.close();
        errors.push(...page.__errors);
        combinations += 1;
      }
      // AND THE FOUR TAILSCALE SCREENS, which no theme sweep reached until this run because the
      // harness reported no tailnet at all.
      for (const tailnet of [
        { state: "absent" },
        { state: "needs-sign-in" },
        { state: "certificates-off", name: "mm1.tail9a3b2.ts.net" },
        {
          state: "ready",
          name: "mm1.tail9a3b2.ts.net",
          origin: "https://mm1.tail9a3b2.ts.net:8443",
          account: "Google as someone@gmail.com",
        },
      ]) {
        const page = await openTheFlow(theme, tailnet);
        await page.close();
        errors.push(...page.__errors);
        combinations += 1;
      }
    }
    assertEqual(errors, [], "the pairing screen raised a page error or logged to console.error");
    return combinations + " combination(s) of theme and state opened with no page error and nothing on console.error";
  });

  await run.check("Connect setup uses managed actions and never asks for Tailscale", async () => {
    const page = await openSheet(browser, "dark", { phoneConnect: { enabled:false,phase:"not-configured" }, phoneTailnet:{state:"absent"} });
    assert(await page.locator("#phone-connect").isVisible(), "Connect setup is missing");
    assert(!await page.locator("#phone-identity").isVisible(), "Connect asks for a Tailscale identity");
    await page.click("#phone-connect-start");
    await page.waitForSelector("#phone-pairing:not([hidden])");
    assert(!await page.locator("#phone-ts-steps").isVisible(), "Managed pairing shows Tailscale steps");
    assert(await page.locator("#phone-connect-pair-help").isVisible(), "Native pairing instructions missing");
    assert((await page.locator("#phone-pair-url").textContent()).startsWith("https://c-00000000000000000000000000000000-g1.richos.ceo/"), "Managed fixture paired through a Tailscale address");
    await page.click("#phone-connect-disable");
    await page.waitForSelector("#phone-connect-start:not([hidden])");
    assertEqual(await page.evaluate(() => window.__RICHOS_CONNECT_ACTIONS__), ["enable","disable"], "Visible controls did not invoke the real action names");
    await page.click("#phone-use-tailnet");
    assert(await page.locator("#phone-identity").isVisible(), "Tailscale route cannot be selected");
    assert(!await page.locator("#phone-connect").isVisible(), "Connect setup remains on the Tailscale route");
    await page.close();
    return "Managed setup, pairing instructions, disable and alternate route controls work in WebKit";
  });

  await run.check("The popup names RichConnect for RichOS and says the CEO's setup sentence (feedback 2026-10-06 item 4)", async () => {
    // His words, verbatim: "Change copy of 'RichOS Connect' instances to 'RichConnect for
    // RichOS'", and the setup line reads "First, set up this RichOS app on your Mac. Then pair
    // your phone with it." Read off the rendered sheet, not the source.
    const page = await openSheet(browser, "dark", { phoneConnect: { enabled:false,phase:"not-configured" }, phoneTailnet:{state:"absent"} });
    const words = await page.evaluate(() => ({
      route: document.getElementById("phone-use-connect").textContent.trim(),
      heading: document.querySelector("#phone-connect .phone-step-title").textContent.trim(),
      start: document.getElementById("phone-connect-start").textContent.trim(),
      stop: document.getElementById("phone-connect-disable").textContent.trim(),
      status: document.getElementById("phone-connect-status").textContent.trim(),
      sheet: document.getElementById("phone-sheet").innerText,
    }));
    assertEqual(words.route, "RichConnect for RichOS", "the route button");
    assertEqual(words.heading, "RichConnect for RichOS", "the Connect heading");
    assertEqual(words.start, "Set up RichConnect for RichOS", "the setup button");
    assertEqual(words.stop, "Turn off RichConnect for RichOS", "the turn-off button");
    assertEqual(words.status, "First, set up this RichOS app on your Mac. Then pair your phone with it.", "the setup sentence");
    assert(!/RichOS Connect/.test(words.sheet), "the popup still says RichOS Connect somewhere: " + words.sheet);
    await page.close();
    return words.status;
  });

  await run.check("Connect keeps a saved pairing visible while the listener recovers", async () => {
    const page = await openSheet(browser, "dark", { phoneConnect:{enabled:true,phase:"active"}, phonePaired:true, phonePairedVia:"connect", phoneListenerStopped:true });
    assert(await page.locator("#phone-paired").isVisible());
    assert(/pairing is saved.*restore the connection/.test(await page.locator("#phone-paired-state").textContent()));
    assert(!await page.locator("#phone-connect-start").isVisible(), "Recovery offers to replace the pairing");
    await page.close(); return "Saved pairing and recovery remain distinct from first use";
  });

  await browser.close();
  const failed = run.report();
  console.log("\nScreenshots: " + SHOTS + " — both themes, both states.");
  console.log(
    failed
      ? "\n" + failed + " check(s) FAILED"
      : "\nthe sheet opens on the one path §61 leaves, nothing is installed on the phone, and " +
        "the code is black on white with a white quiet zone — measured from the pixels, in both themes."
  );
  process.exitCode = failed ? 1 : 0;
})().catch((e) => {
  console.error(e);
  process.exit(1);
});

// ---------------------------------------------------------------------------------------
// MUTATIONS RUN RED, so the checks above are known to be able to fail
// ---------------------------------------------------------------------------------------
//
//  1   ui/qr.js: change one entry of the version table (`dataPerBlock: 28` -> 27)
//        -> check 1 red on every string; the shipped copy and the probe's disagree
//  3   phone.js `paint`: delete the `paper.fillStyle = "#ffffff"; paper.fillRect(...)` pair
//        -> check 3 red with three colors present, one of them transparent — which is exactly
//           the defect that only shows up as "the code will not scan" on his phone in dark mode
//  5   phone.js: move the two warning paragraphs below the first `<canvas>`
//        -> check 5 red; check 4 stays green, which is the point of having both
//  6   phone.js `STEPS`: drop the last entry
//        -> check 6 red at fifteen
//  8   phone.js `render`: treat `pairUrl: null` as still pairing
//        -> check 8 red; an expired window shows a code he can scan and nothing that works
//  9   phone.js: delete the sentence naming VPN and Device Management
//        -> check 9 red
