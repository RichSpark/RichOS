// pointer-drag.test.js — the guest half of pointer-drag.sh, run against mocked CoreGraphics.
//
// What it proves: a left drag posts move, left down, left dragged..., left up; `right: true`
// on one point twice posts move, RIGHT down, RIGHT up and no dragged event (a click, which
// WebKit turns into a `contextmenu`); and the frontmost refusal still sends nothing.
// Added with --right in the candidate 38 walk (2026-10-06). Run: node pointer-drag.test.js
"use strict";
const fs = require("fs");
const path = require("path");
const assert = require("assert");

const SOURCE = fs.readFileSync(path.join(__dirname, "..", "pointer-drag.js"), "utf8");

function drive(params, frontmost) {
  const posted = [];
  const sandbox = {
    ObjC: { import() {} },
    delay() {},
    Application() {
      const proc = { unixId: () => params.pid, frontmost: frontmost };
      return {
        processes: {
          whose(q) {
            if ("frontmost" in q) return frontmost ? [proc] : [{ unixId: () => 1 }];
            return [proc];
          },
        },
      };
    },
  };
  const $ = function () { return null; };
  $.CGEventCreateMouseEvent = (src, type, point, button) => ({ type, button, x: point.x, y: point.y });
  $.CGEventPost = (tap, ev) => posted.push(ev);
  sandbox.$ = $;
  const fn = new Function("ObjC", "delay", "Application", "$", "DRAG_PARAMS",
    SOURCE + "\nreturn run();");
  const answer = JSON.parse(fn(sandbox.ObjC, sandbox.delay, sandbox.Application, sandbox.$, params));
  return { answer, posted };
}

// A left drag, as the divider pull uses it.
{
  const { answer, posted } = drive({ pid: 42, points: [[10, 5], [26, 5]], step: 8, pause: 0, rest: 0 }, true);
  assert.strictEqual(answer.dragged, true);
  assert.strictEqual(answer.button, "left");
  assert.deepStrictEqual(posted.map((e) => e.type), [5, 1, 6, 6, 2]);
  assert.ok(posted.slice(1).every((e) => e.button === 0), "every left event names button 0");
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

console.log("pointer-drag: a left drag, a right-click with no dragged event, a hover with no button and the frontmost refusal passed");
