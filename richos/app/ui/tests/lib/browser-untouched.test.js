// The installed Playwright WebKit is used exactly as Playwright ships it. A postinstall that set
// LSUIElement and re-signed Playwright.app (2026-10-09, removed 2026-10-10) changed its code
// signature. The keychain item "Playwright WebCrypto Master Key" then no longer trusted it, and
// WebKit hung on a keychain prompt the first time a page stored a CryptoKey in IndexedDB. That
// broke the WebKit half of mobile-pwa.test.sh. See the note in browser-reaper.js.
"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");
const { execFileSync } = require("child_process");

let failed = 0;
const check = (name, ok, detail) => {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${!ok && detail ? `\n        ${detail}` : ""}`);
  if (!ok) failed++;
};

const pkg = JSON.parse(fs.readFileSync(path.join(__dirname, "..", "package.json"), "utf8"));
check(
  "the UI tests' postinstall only installs WebKit and modifies no installed browser",
  pkg.scripts && pkg.scripts.postinstall === "playwright install webkit",
  `postinstall is ${JSON.stringify(pkg.scripts && pkg.scripts.postinstall)}`
);

// Where Playwright keeps its browsers, resolved the way Playwright resolves it.
const configured = process.env.PLAYWRIGHT_BROWSERS_PATH;
const root =
  configured && configured !== "0"
    ? configured
    : configured === "0"
      ? path.join(__dirname, "..", "node_modules", "playwright-core", ".local-browsers")
      : path.join(os.homedir(), "Library", "Caches", "ms-playwright");
const bundles = fs.existsSync(root)
  ? fs
      .readdirSync(root)
      .filter((n) => /^webkit-/.test(n))
      .map((n) => path.join(root, n, "Playwright.app", "Contents", "Info.plist"))
      .filter((p) => fs.existsSync(p))
  : [];
for (const plist of bundles) {
  // `plutil -p` reads XML and binary property lists alike.
  const marked = /"LSUIElement"/.test(execFileSync("plutil", ["-p", plist], { encoding: "utf8" }));
  check(
    `installed ${path.basename(path.dirname(path.dirname(path.dirname(plist))))} is Playwright's original (no LSUIElement)`,
    !marked,
    "restore that Playwright.app from Playwright's own download; a modified bundle hangs WebKit on a keychain prompt"
  );
}

process.exitCode = failed ? 1 : 0;
