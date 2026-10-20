// NO TEST WRITES INTO THE TRACKED TREE IT IS VERIFYING.
//
// 2026-09-29: the merge gate verifying `cc/echo-opus-speckle1` in the main checkout was
// refused by its own tests. `splash.js` photographed two app screens the branch had
// legitimately changed and `publishShot` wrote the new pictures over the committed references;
// the gate saw its source change during verification and canceled every check. The nightly
// had been doing the same with `home.js` and restoring it afterwards. Evidence: proof run
// `/Volumes/E1TB/state/richos/proof-runs/60e53bf487d5/20260929T104103Z-6uv1121i/`
// (`03-splash.js.log` lines 2-3, `contamination/source.json`).
//
// WHAT THIS SUITE PROVES, each check a way the fix could be wrong:
//
//   1. A picture that differs from its committed reference is NOT written over it, through the
//      real `publishShot` and a real committed file; the picture this run took is kept in
//      `.shots/changed/` (gitignored) where a person can look at it.
//   2. The harness's end-of-suite guard turns a suite that DOES write a tracked file red, in
//      that suite's own report, naming the file — the layer that catches the next writer that
//      never goes near `publishShot`.
//   3. Negative control: a suite that writes nothing passes the guard.
//   4. A file already modified before the suite started is not blamed for the person's edit;
//      the same file changed again during the suite is.
//   5. A new untracked file is blamed; a gitignored one is not.
//   6. A tracked file saved OUTSIDE the UI tree during the suite (an unrelated editor save) is
//      not blamed on it; one inside the UI tree still is. In a real checkout the watched scope
//      is `richos/app/ui`, not the repository (part-2 hunt section 07, 2026-09-29).
//
// Checks 2-6 run a child suite (`fixtures/tracked-tree-suite.js`) against a throwaway git
// repository, so the guard is proven red without ever touching a real tracked file — a test
// of this rule that wrote into this checkout would be the defect it tests for.

"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");
const zlib = require("zlib");
const { spawnSync, execFileSync } = require("child_process");
const { publishShot, createRun, assert, assertEqual, discardChangedShotsForSelfTest } = require("./lib/harness");

const FIXTURE = path.join(__dirname, "fixtures", "tracked-tree-suite.js");
const GUARD = "the tracked tree is exactly as this suite found it";

/// A tiny valid RGB PNG, in `lib/png.js`'s accepted shape (the same builder
/// `lib/shot-stability.js` self-tests with).
function tinyPng(value) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(4, 0);
  ihdr.writeUInt32BE(4, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  const raw = Buffer.alloc(4 * (4 * 3 + 1));
  let p = 0;
  for (let y = 0; y < 4; y++) {
    raw[p++] = 0;
    for (let x = 0; x < 12; x++) raw[p++] = (value + x * 17 + y * 5) & 0xff;
  }
  const chunk = (type, data) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    return Buffer.concat([len, Buffer.from(type, "ascii"), data, Buffer.alloc(4)]);
  };
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", zlib.deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

function git(cwd, args) {
  return execFileSync("git", args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
}

/// A throwaway repository: `tracked.txt` and `dirty.txt` tracked, `scratch/` ignored.
///
/// TRACKED BY `git add`, NEVER BY A COMMIT. A file in the index is tracked — `git status` reports
/// a change to it exactly as it would to a committed one — and a commit here would run this
/// machine's global commit hooks (the identity guard refuses any other author, rightly) inside
/// a repository that exists for a second.
function throwawayRepo(dirs) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "richos-tracked-tree-"));
  dirs.push(dir);
  git(dir, ["init", "-q"]);
  fs.writeFileSync(path.join(dir, "tracked.txt"), "committed\n");
  fs.writeFileSync(path.join(dir, "dirty.txt"), "committed\n");
  fs.writeFileSync(path.join(dir, ".gitignore"), "scratch/\n");
  git(dir, ["add", "-A"]);
  return dir;
}

function runFixture(root, mode, scope) {
  const env = Object.assign({}, process.env, { RICHOS_UI_TRACKED_TREE_ROOT: root });
  delete env.RICHOS_UI_TRACKED_TREE_SCOPE;
  if (scope) env.RICHOS_UI_TRACKED_TREE_SCOPE = scope;
  // The child is not a suite of this run: it must not write into the run's evidence ledger,
  // and a regeneration asked of THIS process is not asked of it.
  delete env.RICHOS_UI_TESTS_LEDGER;
  delete env.RICHOS_SHOTS_REGENERATE;
  const r = spawnSync(process.execPath, [FIXTURE, mode], { env, encoding: "utf8", timeout: 60000 });
  return { code: r.status, out: (r.stdout || "") + (r.stderr || "") };
}

function guardLine(out) {
  const at = out.indexOf(GUARD);
  if (at < 0) return null;
  const start = out.lastIndexOf("\n", at) + 1;
  const end = out.indexOf("\n  ", at + GUARD.length);
  return out.slice(start, end < 0 ? undefined : end);
}

async function main() {
  const run = createRun("the tracked tree — no test writes into the checkout it is verifying");
  const dirs = [];
  try {
    await run.check("1  a picture that differs from its committed reference is not written over it", async () => {
      // THIS CHECK'S OWN REFERENCE, NOT ANOTHER SUITE'S (hunt part 2, finding 27). It used to
      // aim at the real `shots-5c/5c-02-after-a-plain-decline.png`, and a changed real picture
      // `corrections.js` had just kept at the same `.shots/changed/<key>` was overwritten by
      // this check's 4x4 fixture and then deleted. The destination is inside the tests
      // directory and not scratch, which is all `publishShot` needs to treat it as a
      // reference; the name is unique to this process so neither a real candidate nor a
      // concurrent run can share it.
      const rel = path.join(".generated-references", "tracked-tree-check-" + process.pid, "reference.png");
      const reference = path.join(__dirname, rel);
      const kept = path.join(__dirname, ".shots", "changed", rel);
      fs.mkdirSync(path.dirname(reference), { recursive: true });
      fs.writeFileSync(reference, tinyPng(200));
      try {
        return await checkOneAgainst(rel, reference, kept);
      } finally {
        fs.rmSync(path.dirname(reference), { recursive: true, force: true });
        fs.rmSync(path.dirname(kept), { recursive: true, force: true });
      }
    });

    async function checkOneAgainst(rel, reference, kept) {
      const original = fs.readFileSync(reference);
      const fresh = tinyPng(40);
      const regen = process.env.RICHOS_SHOTS_REGENERATE;
      delete process.env.RICHOS_SHOTS_REGENERATE;
      let after;
      try {
        publishShot(fresh, reference);
      } finally {
        // This check's synthetic picture is not a regression in the suite's own report.
        discardChangedShotsForSelfTest();
        if (regen !== undefined) process.env.RICHOS_SHOTS_REGENERATE = regen;
        after = fs.readFileSync(reference);
        // If the rule is broken this check has just written a committed file. Put it back
        // before anything else reads the tree, then fail.
        if (!after.equals(original)) fs.writeFileSync(reference, original);
      }
      assert(
        after.equals(original),
        "publishShot wrote a changed picture over the committed " + rel + " (" + original.length + " -> " +
          after.length + " bytes). That is the write that made the merge gate refuse itself on 2026-09-29."
      );
      assert(fs.existsSync(kept), "the picture this run took was thrown away rather than kept at " + kept);
      assert(fs.readFileSync(kept).equals(fresh), "the kept picture is not the one this run took: " + kept);
      // It is this check's synthetic picture, not a real candidate; the caller removes it.
      return rel + " untouched (" + original.length + " bytes); this run's picture kept at .shots/changed/" + rel;
    }

    await run.check("2  a suite that writes a tracked file fails its own report, naming the file", async () => {
      const root = throwawayRepo(dirs);
      const r = runFixture(root, "modify");
      const line = guardLine(r.out);
      assert(line, "the child suite rewrote tracked.txt and its report has no line about the tracked tree:\n" + r.out);
      assert(/^\s*FAIL\s/.test(line), "the guard did not fail a suite that rewrote a tracked file: " + JSON.stringify(line));
      assert(r.out.includes("tracked.txt"), "the failure does not name the file it is about:\n" + r.out);
      assertEqual(r.code, 1, "the child suite's exit code");
      return "exit 1, FAIL names tracked.txt";
    });

    await run.check("3  negative control: a suite that writes nothing passes the guard", async () => {
      const root = throwawayRepo(dirs);
      const r = runFixture(root, "clean");
      const line = guardLine(r.out);
      assert(line && /^\s*PASS\s/.test(line), "the guard did not PASS a suite that wrote nothing:\n" + r.out);
      assertEqual(r.code, 0, "the child suite's exit code");
      return "exit 0, guard PASS";
    });

    await run.check("4  an edit made before the suite started is not blamed; the same file changed during it is", async () => {
      const a = throwawayRepo(dirs);
      fs.writeFileSync(path.join(a, "dirty.txt"), "a person's edit, before the run\n");
      const untouched = runFixture(a, "predirty-untouched");
      const l1 = guardLine(untouched.out);
      assert(l1 && /^\s*PASS\s/.test(l1), "a file dirty BEFORE the suite was blamed on it:\n" + untouched.out);
      assertEqual(untouched.code, 0, "exit code with a pre-existing edit left alone");

      const b = throwawayRepo(dirs);
      fs.writeFileSync(path.join(b, "dirty.txt"), "a person's edit, before the run\n");
      const changed = runFixture(b, "predirty-changed");
      const l2 = guardLine(changed.out);
      assert(l2 && /^\s*FAIL\s/.test(l2), "a dirty file the suite changed AGAIN was not blamed:\n" + changed.out);
      assert(changed.out.includes("dirty.txt"), "the failure does not name dirty.txt:\n" + changed.out);
      assertEqual(changed.code, 1, "exit code with a pre-existing edit changed again");
      return "left alone: PASS, exit 0 · changed again: FAIL naming dirty.txt, exit 1";
    });

    await run.check("5  a new untracked file is blamed; a gitignored one is not", async () => {
      const a = throwawayRepo(dirs);
      const created = runFixture(a, "untracked");
      const l1 = guardLine(created.out);
      assert(l1 && /^\s*FAIL\s/.test(l1), "a new untracked file in the tree was not blamed:\n" + created.out);
      assert(created.out.includes("new-reference.png"), "the failure does not name new-reference.png:\n" + created.out);

      const b = throwawayRepo(dirs);
      const ignored = runFixture(b, "ignored");
      const l2 = guardLine(ignored.out);
      assert(l2 && /^\s*PASS\s/.test(l2), "a write into gitignored scratch was blamed:\n" + ignored.out);
      assertEqual(ignored.code, 0, "exit code for a gitignored write");
      return "untracked: FAIL naming new-reference.png · gitignored scratch/: PASS";
    });

    await run.check("6  an unrelated tracked file saved during a suite is not blamed on it; one in the UI tree is", async () => {
      // 2026-09-29, part-2 hunt section 07: the guard watched every tracked file in the
      // repository, so one editor save anywhere failed every UI suite running at that moment.
      const scoped = (dirs) => {
        const dir = throwawayRepo(dirs);
        fs.mkdirSync(path.join(dir, "ui"));
        fs.mkdirSync(path.join(dir, "elsewhere"));
        fs.writeFileSync(path.join(dir, "ui", "view.txt"), "committed\n");
        fs.writeFileSync(path.join(dir, "elsewhere", "notes.txt"), "committed\n");
        git(dir, ["add", "-A"]);
        return dir;
      };
      const outside = runFixture(scoped(dirs), "modify-outside-scope", "ui");
      const l1 = guardLine(outside.out);
      assert(l1 && /^\s*PASS\s/.test(l1), "a save outside the UI tree failed the suite:\n" + outside.out);
      assertEqual(outside.code, 0, "exit code for a save outside the UI tree");

      const inside = runFixture(scoped(dirs), "modify-inside-scope", "ui");
      const l2 = guardLine(inside.out);
      assert(l2 && /^\s*FAIL\s/.test(l2), "a write inside the UI tree was not blamed:\n" + inside.out);
      assert(inside.out.includes("ui/view.txt"), "the failure does not name ui/view.txt:\n" + inside.out);
      assertEqual(inside.code, 1, "exit code for a write inside the UI tree");

      // And in this real checkout the watched scope IS the UI tree, not the repository.
      const tree = require("./lib/tracked-tree");
      const root = tree.resolveRoot();
      assert(root, "this suite is not inside a git checkout");
      const scope = typeof tree.scopeOf === "function" ? tree.scopeOf(root).tracked : ".";
      assertEqual(scope, path.relative(root, path.resolve(__dirname, "..")), "the tracked scope in this checkout");
      return "outside: PASS, exit 0 · inside: FAIL naming ui/view.txt, exit 1 · real scope " + scope;
    });
  } finally {
    for (const d of dirs) fs.rmSync(d, { recursive: true, force: true });
  }

  process.exitCode = run.report() ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(2);
});
