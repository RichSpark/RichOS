// pointer-drag.test.js — the guest half of pointer-drag.sh, run against mocked CoreGraphics.
//
// What it proves: a left drag posts move, left down, left dragged..., left up; `thenKey` adds that
// key's down and up after the button comes up (RichFiles round 107.1's fixes walk); `right: true`
// on one point twice posts move, RIGHT down, RIGHT up and no dragged event (a click, which
// WebKit turns into a `contextmenu`); and the frontmost refusal still sends nothing.
// Added with --right in the candidate 38 walk (2026-10-06). Run: node pointer-drag.test.js
"use strict";
const fs = require("fs");
const path = require("path");
const assert = require("assert");

const SOURCE = fs.readFileSync(path.join(__dirname, "..", "pointer-drag.js"), "utf8");

function drive(params, frontmost, named) {
  const posted = [];
  const asked = [];
  const sandbox = {
    ObjC: { import() {} },
    delay() {},
    Application() {
      // `named`: the guest's process of that name and its pid (a second app).
      const pid = named ? named.pid : params.pid;
      const proc = { unixId: () => pid, frontmost: frontmost };
      return {
        processes: {
          whose(q) {
            asked.push(q);
            if ("frontmost" in q) return frontmost ? [proc] : [{ unixId: () => 1 }];
            if ("name" in q) return named && q.name === named.name ? [proc] : [];
            return [proc];
          },
        },
      };
    },
  };
  const $ = function () { return null; };
  $.CGEventCreateMouseEvent = (src, type, point, button) => ({ type, button, x: point.x, y: point.y, flags: 0x120000 });
  $.CGEventCreateScrollWheelEvent2 = (src, units, count, d1) => ({ type: 22, units, count, d1, flags: 0 });
  $.CGEventSetFlags = (ev, flags) => { ev.flags = flags; };
  $.CGEventSetIntegerValueField = (ev, field, value) => { if (field === 1) ev.clickState = value; };
  $.CGEventCreateKeyboardEvent = (src, code, down) => ({ type: down ? 10 : 11, code });
  $.CGEventPost = (tap, ev) => posted.push(ev);
  sandbox.$ = $;
  const fn = new Function("ObjC", "delay", "Application", "$", "DRAG_PARAMS",
    SOURCE + "\nreturn run();");
  const answer = JSON.parse(fn(sandbox.ObjC, sandbox.delay, sandbox.Application, sandbox.$, params));
  return { answer, posted, asked };
}

// A left drag, as the divider pull uses it.
{
  const { answer, posted } = drive({ pid: 42, points: [[10, 5], [26, 5]], step: 8, pause: 0, rest: 0 }, true);
  assert.strictEqual(answer.dragged, true);
  assert.strictEqual(answer.button, "left");
  assert.deepStrictEqual(posted.map((e) => e.type), [5, 1, 6, 6, 2]);
  assert.ok(posted.slice(1).every((e) => e.button === 0), "every left event names button 0");
}

// --cmd: a Command-click (the Desktop Forward Delete walk adds items to a Finder selection). Command
// rides on the down and up events only: the move carries no flag, so no key is left down.
{
  const { answer, posted } = drive({ pid: 42, points: [[10, 5], [10, 5]], step: 8, pause: 0, rest: 0, cmd: true }, true);
  assert.strictEqual(answer.cmd, true);
  assert.deepStrictEqual(posted.map((e) => [e.type, e.flags]), [[5, 0], [1, 0x100000], [2, 0x100000]]);
  const plain = drive({ pid: 42, points: [[10, 5], [10, 5]], step: 8, pause: 0, rest: 0 }, true);
  assert.strictEqual(plain.answer.cmd, false);
  assert.ok(plain.posted.every((e) => e.flags === 0), "a plain click clears inherited keyboard flags");
  const behind = drive({ pid: 42, points: [[10, 5], [10, 5]], step: 8, pause: 0, rest: 0, cmd: true }, false);
  assert.strictEqual(behind.answer.error, "notfront");
  assert.strictEqual(behind.posted.length, 0);
}

// Fn can be present on legitimate pointer events after keyboard input.
{
  const { answer, posted } = drive({ pid: 42, points: [[10, 5]], double: true, fn: true }, true);
  assert.strictEqual(answer.doubled, true);
  assert.deepStrictEqual(posted.slice(1).map((e) => e.clickState), [1, 1, 2, 2]);
  assert.ok(posted.every((e) => e.flags === 0x800000), "Fn is the only requested modifier");
}

// A click, then Down pressed from the same guest process after the button comes up.
{
  const { answer, posted } = drive({ pid: 42, points: [[10, 5], [10, 5]], step: 8, pause: 0, rest: 0,
                                     thenKey: 125, thenAfter: 0.1 }, true);
  assert.strictEqual(answer.thenKey, 125);
  assert.deepStrictEqual(posted.map((e) => e.type), [5, 1, 2, 10, 11]);
  assert.deepStrictEqual(posted.slice(3).map((e) => e.code), [125, 125]);
}

// A right-click: one point twice.
{
  const { answer, posted } = drive({ pid: 42, points: [[30, 40], [30, 40]], step: 8, pause: 0, rest: 0, right: true }, true);
  assert.strictEqual(answer.button, "right");
  assert.strictEqual(answer.moves, 0);
  assert.deepStrictEqual(posted.map((e) => e.type), [5, 3, 4], "move, right down, right up, nothing dragged");
  assert.deepStrictEqual(posted.slice(1).map((e) => e.button), [1, 1], "the right button, CGMouseButton 1");
  assert.ok(posted.every((e) => e.x === 30 && e.y === 40));
}

// A hover: only moved events (type 5), no button at any point, ending on the last point.
{
  const { answer, posted } = drive({ pid: 42, points: [[10, 5], [26, 5]], step: 8, pause: 0, rest: 0, hover: true }, true);
  assert.strictEqual(answer.hovered, true);
  assert.strictEqual(answer.dragged, undefined);
  assert.deepStrictEqual(posted.map((e) => e.type), [5, 5, 5], "a move to the start, then the moves along the way: no down, dragged or up");
  assert.deepStrictEqual([posted[0].x, posted[posted.length - 1].x], [10, 26]);
}

// A hover over one point is one move.
{
  const { answer, posted } = drive({ pid: 42, points: [[30, 40]], step: 8, pause: 0, rest: 0, hover: true }, true);
  assert.strictEqual(answer.hovered, true);
  assert.deepStrictEqual(posted.map((e) => e.type), [5]);
}

// The refusal: not frontmost, nothing posted.
{
  const { answer, posted } = drive({ pid: 42, points: [[30, 40], [30, 40]], step: 8, pause: 0, rest: 0, right: true }, false);
  assert.strictEqual(answer.error, "notfront");
  assert.strictEqual(posted.length, 0);
}

// The refusal holds for a hover too.
{
  const { answer, posted } = drive({ pid: 42, points: [[30, 40]], step: 8, pause: 0, rest: 0, hover: true }, false);
  assert.strictEqual(answer.error, "notfront");
  assert.strictEqual(posted.length, 0);
}

// A Control-scroll in a second app, named: one move to the point, then one line-unit wheel event
// per line, each with Control (0x40000) held, sent once that app's own pid is frontmost.
{
  const { answer, posted, asked } = drive({ pid: 42, app: "RichGlance", points: [[840, 520]], step: 8, pause: 0, rest: 0, scroll: -3, ctrl: true },
    true, { name: "RichGlance", pid: 77 });
  assert.strictEqual(answer.scrolled, true);
  assert.strictEqual(answer.pid, 77, "the named app's pid, not the recorded one");
  assert.ok(asked.some((q) => q.name === "RichGlance"), "the process was looked up by its name");
  assert.deepStrictEqual(posted.map((e) => e.type), [5, 22, 22, 22]);
  assert.ok(posted.slice(1).every((e) => e.units === 1 && e.count === 1 && e.d1 === -1 && e.flags === 0x40000));
}

// A plain scroll holds no key.
{
  const { posted } = drive({ pid: 42, points: [[10, 10]], step: 8, pause: 0, rest: 0, scroll: 2 }, true);
  assert.deepStrictEqual(posted.slice(1).map((e) => [e.d1, e.flags]), [[1, 0], [1, 0]]);
}

// The refusals: a named app that is not running, and one that is not frontmost, post nothing.
{
  const gone = drive({ pid: 42, app: "RichGlance", points: [[1, 1]], step: 8, pause: 0, rest: 0, scroll: 1 }, true, null);
  assert.strictEqual(gone.answer.error, "noprocess");
  assert.strictEqual(gone.posted.length, 0);
  const behind = drive({ pid: 42, app: "RichGlance", points: [[1, 1]], step: 8, pause: 0, rest: 0, scroll: 1 }, false, { name: "RichGlance", pid: 77 });
  assert.strictEqual(behind.answer.error, "notfront");
  assert.strictEqual(behind.posted.length, 0);
}

// A double-click: one move, then down/up with click state 1 and down/up with click state 2.
{
  const { answer, posted } = drive({ pid: 42, points: [[645, 316]], step: 8, pause: 0, rest: 0, double: true }, true);
  assert.strictEqual(answer.doubled, true);
  assert.deepStrictEqual(posted.map((e) => [e.type, e.clickState]), [[5, undefined], [1, 1], [2, 1], [1, 2], [2, 2]]);
  const behind = drive({ pid: 42, points: [[645, 316]], step: 8, pause: 0, rest: 0, double: true }, false);
  assert.strictEqual(behind.answer.error, "notfront");
  assert.strictEqual(behind.posted.length, 0);
}

// --as-is (a Dock icon, a window in the background): nothing is raised, nothing is required to be
// frontmost, the click is posted where it is and the answer names what was frontmost before it.
{
  const raised = [];
  const posted = [];
  const front = { unixId: () => 9, name: () => "Finder" };
  const sandbox = {
    Application() {
      return { processes: { whose(q) {
        if ("frontmost" in q) return [front];
        // Any lookup by pid or name would be the start of raising an app: there must be none.
        raised.push(q);
        return [{ unixId: () => 42, set frontmost(v) { raised.push(v); } }];
      } } };
    },
  };
  const $ = function () { return null; };
  $.CGEventCreateMouseEvent = (src, type, point, button) => ({ type, button, x: point.x, y: point.y });
  $.CGEventSetFlags = (ev, flags) => { ev.flags = flags; };
  $.CGEventPost = (tap, ev) => posted.push(ev);
  const fn = new Function("ObjC", "delay", "Application", "$", "DRAG_PARAMS", SOURCE + "\nreturn run();");
  const answer = JSON.parse(fn({ import() {} }, () => {}, sandbox.Application, $,
    { pid: 0, points: [[700, 1020], [700, 1020]], step: 8, pause: 0, rest: 0, asis: true }));
  assert.strictEqual(answer.dragged, true);
  assert.strictEqual(answer.asis, true);
  assert.strictEqual(answer.front_before, "Finder", "the answer names what was frontmost before the click");
  assert.strictEqual(raised.length, 0, "no app was looked up to be raised");
  assert.deepStrictEqual(posted.map((e) => e.type), [5, 1, 2], "move, down, up: a click where the pointer is");
}

console.log("pointer-drag: a Command-click carried Command on its down and up only, and the refusal posted nothing");
console.log("pointer-drag: a click with --as-is raised nothing and named the app in front");
console.log("pointer-drag: a double-click with click states 1 and 2 passed");
console.log("pointer-drag: a left drag, a right-click with no dragged event, a hover with no button, a Control-scroll in a named app and the refusals passed");
