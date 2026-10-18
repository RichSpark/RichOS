// THE JOIN THE OTHER TWO SUITES CANNOT MAKE: render the bytes the BACKEND actually emits,
// with the renderer the CEO actually runs.
//
// `workers.js` and `inspector.js` feed the renderer payloads written by hand. That proves
// the renderer and proves nothing about whether `get_timeline` emits what it reads — one
// disagreement (`active_ms` where the renderer expects `activeMs`, a nested `base` where it
// expects a flat one, `worker` vs `workerRun`) and the timeline is blank against the real
// shell while every mock test stays green.
//
// So this suite does not build a payload at all. It runs
//
//     cargo run --example timeline_payload -- --json
//
// from `app/src-tauri` — which prints the payload from `Ledger` on disk -> `MachineryJournal`
// on disk -> the worker-lifecycle stream -> `Spine::timeline` -> `Timeline::view(Ceo)` ->
// `payload()`, through `timeline_view::timeline_payload`, the same file `get_timeline` calls
// — and renders exactly those bytes.
//
// Skipped, loudly and non-fatally, when cargo is unavailable: a harness that silently passes
// because it could not run is worse than one that says it did not.
//
// ONLY WHEN CARGO IS ABSENT (hunt part 2, finding 18). Until 2026-09-30 every cargo failure took
// that skip, so a compile error in the backend this suite exists to check exited 0, and the land
// (which runs this file directly, not through run.js) recorded it passed. A cargo that runs and
// fails is a red check here; only a cargo that cannot be started at all is a skip.

"use strict";

const { spawnSync } = require("child_process");
const path = require("path");
const { loadPlaywright, openFixture, shot, createRun, skipSuite, assert, assertEqual, UI_DIR } = require("./lib/harness");

const TAURI_DIR = path.resolve(UI_DIR, "..", "src-tauri");

function wirePayload(example = "timeline_payload") {
  const r = spawnSync("cargo", ["run", "--quiet", "--example", example, "--", "--json"], {
    cwd: TAURI_DIR,
    encoding: "utf8",
    env: Object.assign({}, process.env, { PATH: process.env.PATH + ":" + process.env.HOME + "/.cargo/bin" }),
    maxBuffer: 32 * 1024 * 1024,
  });
  if (r.error && r.error.code === "ENOENT") {
    return { ok: false, absent: true, why: r.error.message };
  }
  if (r.error || r.status !== 0) {
    const why = (r.error && r.error.message) || (r.stderr || "").trim().slice(-400);
    return { ok: false, absent: false, why: `cargo exited ${r.status}: ${why}` };
  }
  try {
    return { ok: true, payload: JSON.parse(r.stdout) };
  } catch (e) {
    return { ok: false, absent: false, why: "the payload is not JSON: " + e.message };
  }
}

async function main() {
  const run = createRun("real backend bytes, real renderer, WebKit");

  const wire = wirePayload();
  if (wire.absent) {
    // The skip is now RECORDED as well as printed. It read identically to a human before and
    // was invisible to `run.js`, which saw only exit 0 — so a CI runner with no cargo would
    // have counted this suite as green while it did nothing at all. `run.js` fails on a skip
    // it was not explicitly told to allow, and proof-run.py records it NOT RUN.
    skipSuite(
      "real backend bytes, real renderer, WebKit",
      "could not run `cargo run --example timeline_payload` from app/src-tauri. Install Rust " +
        "and re-run.\n        " + wire.why
    );
    return 0;
  }

  await run.check("the backend emits the timeline payload (cargo run --example timeline_payload)", async () => {
    if (!wire.ok) throw new Error(wire.why);
    return `${wire.payload.items.length} item(s)`;
  });
  if (!wire.ok) return run.report();

  await run.check("the backend emits retained machinery through its real command", async () => {
    const machinery = wirePayload("machinery_payload");
    assert(machinery.ok, machinery.why);
    assertEqual(machinery.payload.state, "recorded", "recorded evidence must remain readable");
    assert(machinery.payload.rowCount > 0, "no recorded machinery reached the command");
    assert(machinery.payload.timeline.items.some((item) => item.kind === "activity"), "tool activity disappeared");
    return `${machinery.payload.rowCount} recorded machinery row(s)`;
  });

  const { webkit } = loadPlaywright();
  const browser = await webkit.launch();
  const page = await openFixture(browser);

  await run.check("the wire carries worker rows at all", async () => {
    const kinds = wire.payload.items.map((i) => i.kind);
    const workers = wire.payload.items.filter((i) => i.kind === "worker_activity");
    assertEqual(workers.length, 3, "three delegations, three worker rows: " + kinds.join(", "));
    return workers.map((w) => `${w.worker.workerName}/${w.worker.state}`).join(", ");
  });

  await run.check("the renderer draws every worker the backend sent — none dropped", async () => {
    const r = await page.evaluate((snap) => {
      window.__render(snap, {});
      // Expand every turn, so nothing is hidden by the §6.4 collapse.
      for (const t of window.RichTimeline.turnsOf(window.__model)) window.__model.expanded.add(t.turnId);
      window.__renderOnly();
      const turns = window.RichTimeline.turnsOf(window.__model);
      return {
        chips: Array.from(document.querySelectorAll(".tl-chip")).map((c) => ({
          name: c.querySelector(".tl-chip-name").textContent,
          role: c.querySelector(".tl-chip-role") ? c.querySelector(".tl-chip-role").textContent : null,
          state: c.querySelector(".tl-chip-state").textContent,
        })),
        groups: document.querySelectorAll(".tl-workers").length,
        summary: document.querySelector(".tl-workers-head .tl-activity-text").textContent,
        // NOTHING in the payload may be silently undrawn.
        unrendered: turns.flatMap((t) => t.unrendered.map((i) => i.kind)),
        body: document.body.innerText,
      };
    }, wire.payload);

    const sent = wire.payload.items.filter((i) => i.kind === "worker_activity");
    assertEqual(r.chips.length, sent.length, "every worker on the wire got a chip");
    assertEqual(
      r.chips.map((c) => c.name),
      sent.map((w) => w.worker.workerName),
      "the names on screen are the names on the wire"
    );
    assertEqual(
      r.chips.map((c) => c.role),
      sent.map((w) => w.worker.agentType),
      "and so are the roles"
    );
    assertEqual(r.unrendered, [], "no item in the real payload is silently undrawn");
    assertEqual(r.groups, 1, "three consecutive delegations are one row");
    return `"${r.summary}" — ${r.chips.map((c) => c.name + "/" + c.state).join(", ")}`;
  });

  await run.check("the field names agree: the renderer read every one it needed", async () => {
    // The shape-mismatch class this suite exists for. If the backend renamed `workerName` to
    // `worker_name`, or nested the base, every chip above would read "A teammate" and the
    // states would all be "Status unavailable" — passing shape checks and failing the CEO.
    const r = await page.evaluate(() =>
      Array.from(document.querySelectorAll(".tl-chip")).map((c) => c.getAttribute("aria-label"))
    );
    for (const label of r) {
      assert(!label.startsWith("A teammate"), "a name failed to read off the wire: " + label);
      assert(!label.includes("Status unavailable"), "a state failed to read off the wire: " + label);
    }
    return r.join("\n          ");
  });

  await run.check("`run_ended` renders as the chosen wording, from real bytes", async () => {
    const ended = wire.payload.items.find(
      (i) => i.kind === "worker_activity" && i.worker.observedState === "run_ended"
    );
    assert(ended, "the fixture contains an ended run");
    const r = await page.evaluate((agentId) => {
      const chip = document.getElementById("chip:" + agentId);
      return {
        state: chip.querySelector(".tl-chip-state").textContent,
        qualifier: chip.querySelector(".tl-chip-qualifier").textContent,
        label: chip.getAttribute("aria-label"),
      };
    }, ended.worker.agentId);
    assertEqual(r.state, "Ended");
    assertEqual(r.qualifier, "outcome not recorded");
    return `${ended.worker.workerName}: wire observedState="run_ended", state="${ended.worker.state}" -> "${r.state} · ${r.qualifier}"`;
  });

  await run.check("the inspector reads the same bytes", async () => {
    const ended = wire.payload.items.find(
      (i) => i.kind === "worker_activity" && i.worker.observedState === "run_ended"
    );
    const r = await page.evaluate((w) => {
      const host = document.createElement("div");
      host.id = "insp-host";
      document.body.appendChild(host);
      host.appendChild(window.RichTimeline.renderWorkerInspector(w, { chronologyOpen: true }));
      return host.innerText;
    }, ended.worker);
    assert(r.includes(ended.worker.latestUpdate), "the worker's authored words, verbatim from the wire");
    assert(r.includes("not recorded"), "and the time it did not record");
    assert(!/\b\d+m \d+s\b/.test(r), "no duration invented from the two timestamps: " + r);
    return r.replace(/\n+/g, " | ");
  });

  await run.check("SCREENSHOT: the real payload, the real renderer", async () => {
    await page.evaluate(() => {
      const host = document.getElementById("insp-host");
      if (host) host.remove();
    });
    const s = await shot(page, "realbytes");
    assert(s.bytes > 3000, "too small to be a render: " + s.bytes);
    return `${s.file} (${s.bytes} bytes)`;
  });

  await run.check("no page errors", async () => {
    assertEqual(page.__errors, []);
    return "0 uncaught errors";
  });

  await page.close();
  await browser.close();
  return run.report();
}

main().then(
  (failed) => process.exit(failed ? 1 : 0),
  (e) => {
    console.error(e);
    process.exit(1);
  }
);
