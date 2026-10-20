// THE MESSAGE A STRANGER READS WHEN THE GATE CLOSES THEIR WORK, AND THE FILE THAT CLAIMS TO
// BE A PHOTOGRAPH OF IT.
//
// `docs/verification/pr-trust-gate-2026-09-05/raw/close-comment-rendered.md` records the exact
// comment `.github/workflows/vouch-pr.yml` posts to an unapproved contributor before closing
// their pull request. The LIVE text is not in that file. It is a heredoc inside the workflow:
//
//     cat > "${RUNNER_TEMP}/unvouched-pr.md" <<'MESSAGE'
//     ...
//     MESSAGE
//
// On 2026-09-05 the CEO rewrote that message BY EDITING THE EVIDENCE FILE, believing it was the
// text. It was not. Editing it alone changes nothing a contributor ever sees and makes the
// record false; the wording was carried back into the heredoc by hand and the block re-rendered
// from the workflow's own bytes. Nothing checked that they agreed — no test, no script, no
// workflow — and the failure is silent in BOTH directions. Edit the evidence: the record lies.
// Edit the heredoc: the record lies. This suite is the thing that now checks.
//
// AND THE SILENT DIRECTION THAT IS WORSE THAN EITHER. Vouch renders the message with Nushell's
// `format pattern`, so a stray `{` or `}` anywhere in the text is a RENDER ERROR — and an
// errored job closes nothing while looking perfectly installed on the Actions tab. That failure
// needs no disagreement between the two files at all: both could carry the same stray brace and
// the gate would quietly stop gating. So the brace invariant is asserted independently of the
// comparison, and it is the one check here that would still be worth running if the evidence
// file did not exist.
//
// ── FIVE THINGS THIS HAS TO GET RIGHT, EACH OF WHICH IS A WAY TO WRITE A CHECK THAT PASSES
//    WHILE THE REAL THING FAILS ───────────────────────────────────────────────────────────
//
// 1. THE BYTES THE RUNNER SEES ARE NOT THE BYTES IN THE FILE. The heredoc sits inside a YAML
//    literal block scalar (`run: |`), indented ten spaces. YAML strips that indentation before
//    the shell ever sees the script, so a naive read of `vouch-pr.yml` compares text that is
//    ten spaces wider than anything vouch will ever open. Check 1 dedents it and then PROVES
//    the dedent two independent ways: by handing the extracted script to a real `bash` with a
//    real `RUNNER_TEMP` and reading back the file the shell actually wrote, and (check 2) by
//    parsing the same workflow with a real YAML parser and demanding the same bytes. It also
//    asserts that the naive read DIFFERS, so the trap is shown to be real rather than assumed.
//
// 2. THE RENDERER IS VOUCH'S, NOT A LOOKALIKE. `format pattern` is a Nushell builtin, and a
//    re-implementation of it that diverges is exactly a check that stays green while the job
//    errors. So the real thing runs: vouch's own `vouch/template.nu` at the commit the workflow
//    PINS, fetched as a source tarball whose sha256 is the one the evidence records, driven by
//    the same `nu` a runner would install.
//
// 3. NOTHING IS TYPED THAT COULD BE READ. The four placeholder names come out of vouch's own
//    `gh-check-pr` source (the record it pipes into `template render`); the four sample values
//    come out of the evidence file's own prose; the pinned commit comes out of the workflow;
//    the tarball digest comes out of the evidence. A typed expectation is a second document to
//    keep true, and this suite exists because the first one went stale. Same rule, same reason,
//    as `docs-claims.js` next to it.
//
// 4. EMPTY IS A FAILURE, NOT A PASS. Every inventory — the placeholder set, the sample record,
//    the evidence's rendered block, the quoted opener in the walkthrough — is asserted non-empty
//    before it is compared. A regex that stops matching must not make a join trivially true.
//
// 5. A PIN THAT MOVES WITHOUT THE EVIDENCE MOVING IS THE SAME BUG ONE LEVEL OUT. The evidence
//    names the commit it was rendered at. Check 4 joins that name to the workflow's `uses:` line
//    and to `raw/vouch-pin-resolution.txt`, so bumping the action without re-rendering fails
//    here rather than being discovered by a stranger.
//
// ── WHAT IT DELIBERATELY DOES NOT COVER ────────────────────────────────────────────────────
//
// That the comment is POSTED. That needs the token, the API and a pull request from a second
// account, and it is §4 of the walkthrough — the maintainer's to run. Everything here is about
// the text; nothing here is evidence that the gate has ever fired. The workflow header and
// `docs/verification/pr-trust-gate-2026-09-05/README.md` both say "configured, not proven", and
// a green run of this suite does not change that sentence.
//
// ── DEPENDENCIES, AND WHY THEY ARE HARD RATHER THAN OPTIONAL ───────────────────────────────
//
// `nu` on PATH (`brew install nushell`; `ui-suite-ci.yml` installs it) and, on the first run,
// network access to fetch 41 KB of vouch source into the gitignored `.vouch/` cache beside this
// file. Neither is allowed to turn into a skip. A suite that quietly passes when it cannot do
// its work is the exact failure this directory has now produced twice — a scanner reporting
// CLEAN over an empty corpus, and a `run.js` reporting "all 4 suites passed" over a suite it was
// not running — so a missing interpreter or an unreachable cache is a RED CHECK naming the
// one-line fix, never a green run over nothing.
//
// No browser: like `docs-claims.js`, this is a plain node script. `run.js` runs every .js in
// this directory, so it is registered by existing.

"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");
const crypto = require("crypto");
const { spawnSync } = require("child_process");
const { createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");

const REPO = path.resolve(UI_DIR, "..", "..", "..");
const WORKFLOW = path.join(REPO, ".github", "workflows", "vouch-pr.yml");
const EVIDENCE_DIR = path.join(REPO, "docs", "verification", "pr-trust-gate-2026-09-05");
const RENDERED = path.join(EVIDENCE_DIR, "raw", "close-comment-rendered.md");
const PIN_RESOLUTION = path.join(EVIDENCE_DIR, "raw", "vouch-pin-resolution.txt");
const WALKTHROUGH = path.join(EVIDENCE_DIR, "README.md");
const CACHE = path.join(__dirname, ".vouch");

// The line the message step opens with. It is the anchor for the whole extraction and it is
// spelled here in pieces so that this comment's own copy of it can never be mistaken for the
// thing being searched for.
const HEREDOC_OPEN = 'cat > "${RUNNER_TEMP}/unvouched-pr.md" <<\'MESSAGE\'';
const HEREDOC_DELIMITER = "MESSAGE";

// Where the evidence file stops explaining itself and starts being a photograph.
const RENDER_MARKER = "--- 8< --- rendered output begins ---";

const read = (p) => fs.readFileSync(p, "utf8");

// ---------------------------------------------------------------------------------------
// Derivations — the workflow side
// ---------------------------------------------------------------------------------------

/// Pull a YAML literal block scalar (`run: |`) out of a workflow the way the spec says a
/// consumer must, and the way GitHub's runner does before it writes the script to disk.
///
/// The rules applied, none of them optional here:
///   * the block is the lines AFTER the `|`, up to the first non-empty line whose indentation
///     is less than the block's own;
///   * with no explicit indentation indicator, the block's indentation is that of its first
///     NON-EMPTY line — an empty line inside the block indents nothing and must not be allowed
///     to set it to zero;
///   * every line has exactly that many leading spaces removed, never more;
///   * default chomping is "clip": one trailing newline, and no more.
///
/// This is a hand-rolled parser and it is not TRUSTED anywhere. Check 1 hands its output to a
/// real shell, and check 2 demands the same bytes from a real YAML parser. If either disagrees,
/// the disagreement is the failure — this function is never the authority.
function extractBlockScalarContaining(yamlText, needle) {
  const lines = yamlText.split("\n");
  const blocks = [];
  for (let i = 0; i < lines.length; i++) {
    if (!/^\s*run:\s*\|\s*$/.test(lines[i])) continue;

    // Indentation is set by the first non-empty line of the block.
    let indent = null;
    for (let j = i + 1; j < lines.length; j++) {
      if (lines[j].trim() === "") continue;
      indent = lines[j].length - lines[j].replace(/^ +/, "").length;
      break;
    }
    if (indent === null) continue;

    const body = [];
    let j = i + 1;
    for (; j < lines.length; j++) {
      const line = lines[j];
      if (line.trim() === "") {
        body.push("");
        continue;
      }
      const lead = line.length - line.replace(/^ +/, "").length;
      if (lead < indent) break;
      body.push(line.slice(indent));
    }
    // "clip": drop trailing empty lines, then restore exactly one newline.
    while (body.length && body[body.length - 1] === "") body.pop();
    blocks.push({ startLine: i + 2, indent, text: body.join("\n") + "\n" });
  }
  return blocks.filter((b) => b.text.includes(needle));
}

/// The heredoc body, sliced structurally out of an already-dedented `run:` script.
///
/// Returns the body AND the shape facts the caller asserts on, rather than asserting here:
/// a parser that also judges is a parser whose failures all look the same.
function sliceHeredoc(script) {
  const lines = script.split("\n");
  const openAt = lines.indexOf(HEREDOC_OPEN);
  const closeAt = lines.indexOf(HEREDOC_DELIMITER);
  const trailing = closeAt < 0 ? [] : lines.slice(closeAt + 1).filter((l) => l !== "");
  return {
    openAt,
    closeAt,
    trailing,
    // Every heredoc line is terminated, including the last one.
    body: openAt >= 0 && closeAt > openAt ? lines.slice(openAt + 1, closeAt).join("\n") + "\n" : null,
  };
}

/// The commit the workflow pins `mitchellh/vouch/action/check-pr` to.
function pinnedCommit(yamlText) {
  const m = yamlText.match(/mitchellh\/vouch\/action\/check-pr@([0-9a-f]{40})\b/);
  return m ? m[1] : null;
}

// ---------------------------------------------------------------------------------------
// Derivations — the evidence side
// ---------------------------------------------------------------------------------------

/// The block the evidence file claims is the rendered output: everything after the marker,
/// with leading and trailing EMPTY lines removed.
///
/// ONLY EMPTY LINES, never whitespace. Twelve of this message's line breaks are markdown hard
/// breaks — a line ending in two spaces — so a rule that trimmed whitespace would erase the
/// difference between a rendered break and a rendered space, which is the one formatting
/// property this text depends on.
function evidenceRenderedBlock(text) {
  const at = text.indexOf(RENDER_MARKER);
  if (at < 0) return null;
  const occurrences = text.split(RENDER_MARKER).length - 1;
  if (occurrences !== 1) return null;
  const after = text.slice(at + RENDER_MARKER.length);
  return after.replace(/^\n+/, "").replace(/\n+$/, "");
}

/// The sample record the evidence says it rendered with, read out of its own prose:
///
///     ... author `some-stranger`, owner `WebDevBooster`, repo `richos`, default branch `main`.
///
/// The KEYS are not supplied by this function — they come from vouch's source — so a key vouch
/// stops passing, or a value the evidence stops naming, is a failed lookup rather than a
/// silently smaller record.
function sampleRecord(text, keys) {
  const flat = text.replace(/\s+/g, " ");
  const out = {};
  for (const key of keys) {
    const label = key.replace(/_/g, " ");
    const m = flat.match(new RegExp(label + " `([^`]+)`"));
    if (m) out[key] = m[1];
  }
  return out;
}

// ---------------------------------------------------------------------------------------
// Derivations — vouch's own source, at the pinned commit
// ---------------------------------------------------------------------------------------

/// The record keys `gh-check-pr` pipes into `template render`, read out of vouch's Nushell.
/// This is the set of placeholders that can legally appear in the message; anything else in
/// braces is a name `format pattern` will not find.
///
/// Scoped to `gh-check-pr` deliberately: `gh-check-issue` builds a record of its own a hundred
/// lines further down, and reading the file as a whole would quietly accept whichever came
/// first if the two ever diverge.
function vouchPrTemplateKeys(vouchDir) {
  const src = read(path.join(vouchDir, "vouch", "github.nu"));
  const defAt = src.search(/^export def .*\bgh-check-pr\b/m);
  if (defAt < 0) return [];
  const rest = src.slice(defAt);
  const nextDef = rest.slice(1).search(/^export def /m);
  const scope = nextDef < 0 ? rest : rest.slice(0, nextDef + 1);
  const m = scope.match(/let message = \{([\s\S]*?)\}\s*\|\s*template render/);
  if (!m) return [];
  return [...m[1].matchAll(/^\s*([a-z_]+):/gm)].map((x) => x[1]);
}

// ---------------------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------------------

function have(cmd) {
  return spawnSync("sh", ["-c", "command -v " + cmd], { encoding: "utf8" }).status === 0;
}

function sha256(buf) {
  return crypto.createHash("sha256").update(buf).digest("hex");
}

/// Vouch's source at one commit, verified against the digest the EVIDENCE records and cached
/// under `.vouch/` so a second run costs nothing.
///
/// The digest is the whole point of fetching a tarball rather than cloning: a clone gives you
/// whatever that ref resolves to today, and this has to be the bytes the evidence was made
/// from. A mismatch is a hard failure and the cached file is removed, so the next run cannot
/// inherit a bad download.
function acquireVouch(commit, expectedDigest) {
  fs.mkdirSync(CACHE, { recursive: true });
  const tarball = path.join(CACHE, commit + ".tar.gz");
  const dir = path.join(CACHE, "vouch-" + commit);

  if (fs.existsSync(tarball) && sha256(fs.readFileSync(tarball)) !== expectedDigest) {
    fs.rmSync(tarball, { force: true });
    fs.rmSync(dir, { recursive: true, force: true });
  }

  if (!fs.existsSync(tarball)) {
    const url = "https://codeload.github.com/mitchellh/vouch/tar.gz/" + commit;
    const got = spawnSync("curl", ["-fsSL", "-o", tarball, url], { encoding: "utf8" });
    assert(
      got.status === 0 && fs.existsSync(tarball),
      "could not fetch vouch's source from " + url + "\n          " +
        "this suite renders the message through vouch's own engine and will not fake it.\n          " +
        (got.stderr || "curl exited " + got.status)
    );
  }

  const actual = sha256(fs.readFileSync(tarball));
  if (actual !== expectedDigest) {
    fs.rmSync(tarball, { force: true });
    throw new Error(
      "vouch source tarball is not the bytes the evidence records\n" +
        "          expected " + expectedDigest + "\n" +
        "          actual   " + actual
    );
  }

  if (!fs.existsSync(path.join(dir, "vouch", "template.nu"))) {
    fs.rmSync(dir, { recursive: true, force: true });
    fs.mkdirSync(dir, { recursive: true });
    const un = spawnSync("tar", ["xzf", tarball, "-C", dir, "--strip-components=1"], { encoding: "utf8" });
    assert(un.status === 0, "could not extract " + tarball + ": " + (un.stderr || un.status));
  }
  return dir;
}

/// Render a template file through vouch's `template render` — the same command `gh-check-pr`
/// calls, imported the same way `github.nu` imports it (`use template.nu`).
///
/// The record crosses into Nushell as JSON rather than as interpolated source, so a sample
/// value carrying a quote cannot become syntax.
function renderThroughVouch(vouchDir, templateFile, record) {
  assert(have("nu"), "nushell is not on PATH — install it (`brew install nushell`) and re-run.\n" +
    "          this check renders through vouch's real engine and there is nothing honest to do without it.");

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "vouch-render-"));
  const cfgPath = path.join(tmp, "record.json");
  const scriptPath = path.join(tmp, "render.nu");
  fs.writeFileSync(cfgPath, JSON.stringify({ record, template: templateFile }));
  fs.writeFileSync(
    scriptPath,
    'use "' + path.join(vouchDir, "vouch", "template.nu") + '"\n' +
      'let cfg = (open --raw "' + cfgPath + '" | from json)\n' +
      "print -n ($cfg.record | template render $cfg.template)\n"
  );

  // `-n` so the operator's own Nushell config, aliases and overlays cannot reach the render.
  const out = spawnSync("nu", ["-n", scriptPath], { encoding: "utf8", maxBuffer: 8 * 1024 * 1024 });
  assert(
    out.status === 0,
    "vouch's `template render` FAILED on the message in the workflow — which on a real run is a\n" +
      "          job that closes nothing while looking installed:\n          " +
      String(out.stderr || "").trim().split("\n").join("\n          ")
  );
  return out.stdout;
}

/// The first place two texts differ, said in a way somebody can act on: line number, both
/// lines, and trailing spaces made visible.
///
/// A WHOLE-TEXT DUMP IS NOT A DIAGNOSIS. `assertEqual` prints both sides in full, which for
/// 1.5 KB of message is two walls of JSON with one word different somewhere inside them —
/// complete, and useless at 3 AM. And the difference most likely to matter here is invisible:
/// twelve of this message's line breaks ARE trailing spaces, so the lines are printed through
/// `JSON.stringify` deliberately rather than raw.
function firstDifference(a, b, labelA, labelB, unit) {
  const la = a.split("\n");
  const lb = b.split("\n");
  const show = (s) => (s === undefined ? "<no such line>" : JSON.stringify(s));
  const pad = Math.max(labelA.length, labelB.length);
  for (let i = 0; i < Math.max(la.length, lb.length); i++) {
    if (la[i] !== lb[i]) {
      return (
        "first difference at line " + (i + 1) + " of the " + unit + "\n" +
        "          " + labelA.padEnd(pad) + "  " + show(la[i]) + "\n" +
        "          " + labelB.padEnd(pad) + "  " + show(lb[i])
      );
    }
  }
  return "the texts differ in length only: " + labelA + " " + a.length + " chars, " + labelB + " " + b.length;
}

// ---------------------------------------------------------------------------------------

async function main() {
  const run = createRun("the pull-request gate's message vs the evidence that records it");

  const workflowText = read(WORKFLOW);
  const evidenceText = read(RENDERED);

  // Shared derivations. Each is computed once and each check asserts on it independently, so a
  // failure names the join that broke rather than the first one that touched it.
  const blocks = extractBlockScalarContaining(workflowText, HEREDOC_OPEN);
  const script = blocks.length === 1 ? blocks[0].text : null;
  const sliced = script ? sliceHeredoc(script) : null;
  const heredoc = sliced ? sliced.body : null;
  const commit = pinnedCommit(workflowText);
  const recordedBlock = evidenceRenderedBlock(evidenceText);

  let vouchDir = null;
  let rendered = null;

  // =====================================================================================
  // 1. The bytes the RUNNER sees, proven by handing the step to a real shell
  // =====================================================================================

  await run.check("the message step is a bare heredoc write, and a real shell produces the bytes claimed", async () => {
    assertEqual(blocks.length, 1, "exactly one `run:` block in the workflow should open the message heredoc");
    assert(
      sliced.openAt >= 0,
      "the dedented step does not contain the heredoc opener on a line of its own:\n          " + HEREDOC_OPEN
    );
    assert(sliced.openAt === 0, "the message step must OPEN with the heredoc write; found it at line " + (sliced.openAt + 1));
    assert(sliced.closeAt > 0, "the heredoc has no closing `" + HEREDOC_DELIMITER + "` delimiter on its own line");
    assertEqual(sliced.trailing, [], "the message step must do nothing after the heredoc closes");
    assert(heredoc && heredoc.length > 0, "EMPTY INVENTORY: the heredoc body is empty");

    // Having asserted the shape, the script is provably ONE `cat` into `$RUNNER_TEMP` with a
    // QUOTED delimiter — no expansion, no substitution, no command anywhere in it. That is
    // what makes running it here safe, and it is asserted before it is run rather than
    // assumed: a future edit that adds a second command to this step fails above, not here.
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "vouch-step-"));
    const stepPath = path.join(tmp, "step.sh");
    fs.writeFileSync(stepPath, script);
    const ran = spawnSync("bash", ["-e", stepPath], {
      encoding: "utf8",
      env: Object.assign({}, process.env, { RUNNER_TEMP: tmp }),
    });
    assert(ran.status === 0, "the workflow's own message step failed under bash: " + (ran.stderr || ran.status));
    const written = read(path.join(tmp, "unvouched-pr.md"));
    assertEqual(written, heredoc, "the file the SHELL writes is not the heredoc body this suite sliced");

    // ...and the trap, shown rather than described. A naive read of the workflow file compares
    // text that is `indent` spaces wider on every line than anything vouch will ever open.
    const naive = script
      .split("\n")
      .slice(1, sliced.closeAt)
      .map((l) => " ".repeat(blocks[0].indent) + l)
      .join("\n") + "\n";
    assert(naive !== written, "the block scalar's indentation is 0, so this suite is not proving what it claims to");

    return (
      written.split("\n").length - 1 + " lines, " + written.length + " bytes, after stripping " +
      blocks[0].indent + " spaces of block-scalar indentation that a naive read of the file would have compared"
    );
  });

  // =====================================================================================
  // 2. The same bytes, from a real YAML parser
  // =====================================================================================

  await run.check("a real YAML parser dedents the step to exactly the same script", async () => {
    assert(have("nu"), "nushell is not on PATH — install it (`brew install nushell`) and re-run.\n" +
      "          without a second parser, the dedent above is this suite's own word for itself.");
    assert(script, "no message step to compare (check 1 says why)");

    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "vouch-yaml-"));
    const outPath = path.join(tmp, "step.sh");
    const nuPath = path.join(tmp, "parse.nu");
    fs.writeFileSync(
      nuPath,
      'let steps = (open --raw "' + WORKFLOW + '" | from yaml | get jobs.vouch.steps)\n' +
        "let hit = ($steps | where ($it.run? | default \"\") =~ 'unvouched-pr.md')\n" +
        'if ($hit | length) != 1 { error make { msg: $"expected one message step, found ($hit | length)" } }\n' +
        'print -n ($hit | first | get run)\n'
    );
    const parsed = spawnSync("nu", ["-n", nuPath], { encoding: "utf8", maxBuffer: 8 * 1024 * 1024 });
    assert(parsed.status === 0, "a real YAML parser could not read the workflow: " + (parsed.stderr || parsed.status));
    fs.writeFileSync(outPath, parsed.stdout);

    assert(
      parsed.stdout === script,
      "this suite's block-scalar dedent disagrees with a real YAML parser — the extraction is wrong,\n" +
        "          so every comparison built on it is comparing the wrong bytes.\n" +
        "          " + firstDifference(script, parsed.stdout, "this suite sliced:", "the YAML parser gave:", "step")
    );
    return "independent parse agrees, " + parsed.stdout.length + " bytes";
  });

  // =====================================================================================
  // 3. The brace invariant — the failure that needs no disagreement at all
  // =====================================================================================

  await run.check("the only braces in the message are the placeholders vouch's own source passes", async () => {
    assert(heredoc, "no heredoc body to check (check 1 says why)");
    assert(commit, "the workflow does not pin `mitchellh/vouch/action/check-pr` to a 40-character commit");
    vouchDir = vouchDir || acquireVouch(commit, expectedTarballDigest(evidenceText));

    const keys = vouchPrTemplateKeys(vouchDir);
    assert(keys.length > 0, "EMPTY INVENTORY: no record keys found in vouch's own `gh-check-pr`");

    const used = [...heredoc.matchAll(/\{([^{}]*)\}/g)].map((m) => m[1]);
    const unknown = [...new Set(used)].filter((k) => !keys.includes(k)).sort();
    assertEqual(unknown, [], "placeholders in the message that vouch's `gh-check-pr` does not pass — each one is a render error");

    // A `{` or `}` that is not part of a placeholder at all. `format pattern` errors on it, and
    // an errored job closes nothing, so this is checked over the text with every legal
    // placeholder removed rather than by trusting the match above to have been exhaustive.
    const residue = heredoc.replace(/\{[^{}]*\}/g, "");
    const stray = [];
    residue.split("\n").forEach((line, i) => {
      if (/[{}]/.test(line)) stray.push("line " + (i + 1) + ": " + JSON.stringify(line));
    });
    assertEqual(stray, [], "stray braces in the message — `format pattern` errors on these, and an errored job closes NOTHING");

    assert(used.length > 0, "EMPTY INVENTORY: the message contains no placeholder at all, so this check proved nothing");
    return used.length + " placeholder uses over " + new Set(used).size + " names, all in vouch's record: " + keys.join(", ");
  });

  // =====================================================================================
  // 4. The pin the evidence was rendered at IS the pin the workflow carries
  // =====================================================================================

  await run.check("the commit the evidence names is the commit the workflow pins, in both raw files", async () => {
    assert(commit, "the workflow does not pin the action to a 40-character commit");

    const inRendered = (evidenceText.match(/\b[0-9a-f]{40}\b/g) || []);
    assert(inRendered.length > 0, "EMPTY INVENTORY: `raw/close-comment-rendered.md` names no commit, so re-rendering it cannot be checked");
    assert(
      inRendered.includes(commit),
      "the evidence was rendered at a different commit than the workflow now pins\n" +
        "          workflow pins  " + commit + "\n" +
        "          evidence names " + [...new Set(inRendered)].join(", ")
    );

    const pinText = read(PIN_RESOLUTION);
    const inPin = pinText.match(/check-pr@([0-9a-f]{40})/);
    assert(inPin, "EMPTY INVENTORY: `raw/vouch-pin-resolution.txt` states no pinned reference");
    assertEqual(inPin[1], commit, "the pin-resolution transcript names a commit the workflow no longer pins");

    return "commit " + commit.slice(0, 12) + " agrees across the workflow and both raw records";
  });

  // =====================================================================================
  // 5. Vouch's own engine, at that commit, renders the message
  // =====================================================================================

  await run.check("vouch's own `template render`, at the pinned commit, renders the message without erroring", async () => {
    assert(heredoc, "no heredoc body to render (check 1 says why)");
    assert(commit, "no pinned commit to render at (check 4 says why)");
    vouchDir = vouchDir || acquireVouch(commit, expectedTarballDigest(evidenceText));

    const keys = vouchPrTemplateKeys(vouchDir);
    const record = sampleRecord(evidenceText, keys);
    const missing = keys.filter((k) => !(k in record)).sort();
    assertEqual(missing, [], "the evidence does not say what value it rendered these keys with, so the render cannot be reproduced");

    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "vouch-tpl-"));
    const templateFile = path.join(tmp, "unvouched-pr.md");
    fs.writeFileSync(templateFile, heredoc);
    rendered = renderThroughVouch(vouchDir, templateFile, record);

    assert(rendered.length > 0, "EMPTY INVENTORY: the render produced nothing");
    for (const k of keys) {
      assert(!rendered.includes("{" + k + "}"), "`{" + k + "}` survived the render — it was not substituted");
    }
    return "rendered " + rendered.length + " bytes with " + keys.map((k) => k + "=" + record[k]).join(" ");
  });

  // =====================================================================================
  // 6. ...and it is byte-for-byte the block the evidence records
  // =====================================================================================

  await run.check("the render is byte-for-byte the block recorded in the evidence", async () => {
    assert(rendered !== null, "nothing was rendered (check 5 says why)");
    assert(
      recordedBlock !== null,
      "`raw/close-comment-rendered.md` does not carry exactly one `" + RENDER_MARKER + "` line, so there is no block to compare"
    );
    assert(recordedBlock.length > 0, "EMPTY INVENTORY: the evidence's rendered block is empty");

    // Both sides lose trailing EMPTY lines and nothing else. The evidence file ends with a
    // blank line because it is a text file; the render ends with the heredoc's own last
    // newline. Trailing SPACES are never touched — twelve of this message's line breaks are
    // trailing spaces, and erasing them would erase the difference between a rendered line
    // break and a rendered space.
    const a = rendered.replace(/\n+$/, "");
    const b = recordedBlock.replace(/\n+$/, "");
    assert(
      a === b,
      "THE EVIDENCE AND THE WORKFLOW DISAGREE. One of them was edited without the other.\n" +
        "          The text a closed-out contributor actually reads is the heredoc in\n" +
        "          .github/workflows/vouch-pr.yml; the file under docs/verification is only a record of it.\n" +
        "          " + firstDifference(a, b, "rendered from the workflow:", "recorded in the evidence:", "message")
    );
    return a.split("\n").length + " lines identical, " + a.length + " bytes";
  });

  // =====================================================================================
  // 7. The walkthrough quotes the message's real opening line
  // =====================================================================================
  //
  // §4 step 3 of the walkthrough tells the maintainer what a CORRECT result looks like, by
  // quoting the first line of the comment. That quote is a claim about the same heredoc, in a
  // second file, and on 2026-09-05 it was already stale: it still carried the opener from
  // before the CEO's rewrite. A stale "correct result" is worse than none — it is an
  // instruction to read a real pass as a failure.

  await run.check("the walkthrough's quoted opening line is the message's actual opening line", async () => {
    assert(rendered !== null, "nothing was rendered (check 5 says why)");
    const walkthrough = read(WALKTHROUGH);
    const quoted = walkthrough.match(/comment beginning "([^"]+)"/);
    assert(quoted, 'EMPTY INVENTORY: the walkthrough no longer quotes the comment\'s opening line ("comment beginning \\"…\\"")');

    // The walkthrough writes the account as a placeholder, because the reader supplies it.
    const keys = vouchPrTemplateKeys(vouchDir);
    const record = sampleRecord(evidenceText, keys);
    const expected = quoted[1].replace(/<[^>]+>/, record.author);

    // Compared on WORDS, not bytes: the walkthrough is prose and wraps its quote across lines,
    // and the message's own hard line breaks are trailing spaces. Check 6 owns the bytes. What
    // this one owns is that the maintainer is told to expect the sentence the bot really sends
    // — the failure being an instruction to read a correct pass as a failure.
    const flat = (s) => s.replace(/\s+/g, " ").trim();
    assert(
      flat(rendered).startsWith(flat(expected)),
      "the walkthrough tells the maintainer to expect an opening line the message does not have\n" +
        "          walkthrough says " + JSON.stringify(flat(expected)) + "\n" +
        "          message begins   " + JSON.stringify(rendered.split("\n")[0])
    );
    return JSON.stringify(flat(expected));
  });

  const failed = run.report();
  return failed;
}

/// The sha256 of vouch's source tarball at the pinned commit, as the EVIDENCE records it.
///
/// Read rather than typed for the same reason as everything else here: it is a claim the
/// evidence makes about a program, and a claim nothing joins to reality is a claim that decays.
/// Kept as a function rather than folded into the acquire step so that a missing digest fails
/// with its own sentence instead of as a mysterious download mismatch.
function expectedTarballDigest(evidenceText) {
  const m = evidenceText.match(/sha256\s*`?([0-9a-f]{64})`?/);
  assert(
    m,
    "`raw/close-comment-rendered.md` does not record the sha256 of the vouch source it rendered with,\n" +
      "          so the engine this check runs cannot be pinned to the one the evidence used."
  );
  return m[1];
}

main().then((f) => { process.exitCode = f ? 1 : 0; });

// ---------------------------------------------------------------------------------------
// EVERY CHECK ABOVE WAS RUN RED, on 2026-09-05, before it was believed
// ---------------------------------------------------------------------------------------
//
// A green suite proves nothing about a check that could not fail. Each mutation below was
// applied to the real tree, run, and reverted; the suite was green before each and green again
// after. `#` is the check that went red and had to.
//
//  1. #6, #7  one word in the HEREDOC — "sorry about the abrupt landing" -> "apologies about
//             the abrupt landing". This is the case the whole suite exists for: the workflow
//             moves, the evidence does not, and today nothing noticed.
//  2. #6      one word in the EVIDENCE block — "still in your fork" -> "still in your clone".
//             The other direction, and the one the CEO actually took: edit the record, change
//             nothing a contributor reads. Reported at line 22 of the message.
//  3. #3,#5,  a stray brace — "That's it." -> "That's it {.". Check 3 named the line; check 5
//     #6,#7   showed vouch's real engine failing with `nu::shell::delimiter_error`, which on a
//             live run is a job that closes nothing while looking installed. This mutation is
//             why the brace invariant is a check of its own: both files could carry it and
//             agree.
//  4. #4      the pin moved to a commit the evidence was not rendered at. Check 4 named the
//             disagreement; 3 and 5 failed too, because a pin nobody published cannot be
//             fetched, and that is the honest outcome rather than a fallback to a cached tree.
//  5. #1      a second command (`echo staged`) added after the heredoc closes. Nothing else
//             went red — the message is unchanged — which is the point: the step stops being a
//             pure write, and only the check that owns the shape says so.
//  6. #1,#2   the block-scalar dedent broken in THIS FILE (`body.push(line)` instead of
//             `body.push(line.slice(indent))`). Check 2 caught it against a real YAML parser.
//             Without that mutation, check 2 is a claim about a parser nobody has tested.
//  7. #3,#5   the evidence stopped recording the tarball sha256. The engine cannot be pinned to
//             the one the evidence used, so the checks refuse rather than fetch whatever is
//             served.
//  8. #4      the evidence stopped naming the commit it was rendered at — EMPTY INVENTORY,
//             failed rather than passing vacuously. Note that 5 and 6 stayed green: the render
//             still matched. That is exactly the silent state check 4 exists to refuse.
//  9. #7      the walkthrough's quoted opener drifted. It did not need a mutation to go red the
//             first time — it was already stale on the committed tree, still telling the
//             maintainer to expect "Hi @<account>, and thank you for this" from before the
//             CEO's rewrite. A stale "correct result" is worse than none: it is an instruction
//             to read a real pass as a failure. Fixed in the same commit.
