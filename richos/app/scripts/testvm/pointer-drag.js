// pointer-drag.js — the guest half of pointer-drag.sh. Runs under `osascript -l JavaScript`, with
// a `var DRAG_PARAMS = {...};` block prepended by the host (built by python3, never interpolated).
//
// A real mouse drag, in CoreGraphics events, the way a hand drags a divider: the pointer goes to
// the first point, the left button goes down, the pointer moves through every point given in
// small steps a hand would take, and the button comes up at the last one. The app receives
// ordinary pointer events; nothing about the app is bypassed. hand-file.js drags a FILE from
// Finder; this drags whatever is under the pointer inside the app (the Output panel's divider,
// output side-panel PRD §12.9).
//
// Refused unless the app under test is FRONTMOST by PID first — the discipline ax.sh has carried
// since a synthetic Command-Q reached the operator's Terminal on 2026-09-19. Runs only inside the
// owned test guest.
ObjC.import("CoreGraphics");

function run() {
  var P = DRAG_PARAMS;
  var se = Application("System Events");
  function out(o) { return JSON.stringify(o); }
  function error(code, why) { return out({ error: code, reason: String(why) }); }
  function frontPid() {
    var f = se.processes.whose({ frontmost: true });
    return f.length ? f[0].unixId() : 0;
  }
  var procs = se.processes.whose({ unixId: P.pid });
  if (!procs.length) return error("noprocess", "no process with pid " + P.pid);
  var proc = procs[0];
  proc.frontmost = true;
  for (var i = 0; i < 20 && frontPid() !== P.pid; i++) delay(0.1);
  if (frontPid() !== P.pid) return error("notfront", "refusing: the app under test is not frontmost, so nothing was dragged");

  // P.right: the RIGHT button (CGMouseButton 1). Given one point twice it is a real right-click,
  // a `contextmenu` event in the app (the Output panel row's menu, PRD §6.3).
  var button = P.right ? 1 : 0;
  function mouse(type, x, y) {
    var ev = $.CGEventCreateMouseEvent($(), type, { x: x, y: y }, type === 5 ? 0 : button);
    $.CGEventPost(0, ev);
  }
  var pts = P.points;
  // P.hover: the pointer only MOVES (CGEventType 5, no button) through the points, resting at each, the
  // way a hand hovers. A tooltip that must appear "at once" is shown by `mouseover`, which a drag with
  // the button held does not deliver. Added with the nightly 68 walk (the "+ New chat" tooltip).
  if (P.hover) {
    mouse(5, pts[0][0], pts[0][1]);
    delay(0.25);
    var moved = 0;
    for (var h = 1; h < pts.length; h++) {
      var gx = pts[h - 1][0], gy = pts[h - 1][1], ux = pts[h][0], uy = pts[h][1];
      var span = Math.max(Math.abs(ux - gx), Math.abs(uy - gy));
      var n = Math.max(1, Math.ceil(span / P.step));
      for (var q = 1; q <= n; q++) {
        mouse(5, gx + (ux - gx) * q / n, gy + (uy - gy) * q / n);
        moved += 1;
        delay(P.pause);
      }
      delay(P.rest);
    }
    return out({ hovered: true, points: pts, moves: moved, pid: P.pid });
  }
  // 5 moved; left: 1 down, 6 dragged, 2 up; right: 3 down, 7 dragged, 4 up (CGEventType).
  var DOWN = P.right ? 3 : 1, DRAGGED = P.right ? 7 : 6, UP = P.right ? 4 : 2;
  mouse(5, pts[0][0], pts[0][1]);
  delay(0.25);
  mouse(DOWN, pts[0][0], pts[0][1]);
  delay(0.25);
  var sent = 0;
  for (var k = 1; k < pts.length; k++) {
    var fx = pts[k - 1][0], fy = pts[k - 1][1], tx = pts[k][0], ty = pts[k][1];
    var dist = Math.max(Math.abs(tx - fx), Math.abs(ty - fy));
    var steps = Math.max(1, Math.ceil(dist / P.step));
    for (var s = 1; s <= steps; s++) {
      // A right-click on one point moves nothing: no dragged event, so the app sees a click.
      if (dist > 0) mouse(DRAGGED, fx + (tx - fx) * s / steps, fy + (ty - fy) * s / steps);
      if (dist > 0) sent += 1;
      delay(P.pause);
    }
    // A hand rests at each point it was asked to reach before it moves on.
    delay(P.rest);
  }
  var last = pts[pts.length - 1];
  mouse(UP, last[0], last[1]);
  delay(0.3);
  return out({ dragged: true, button: P.right ? "right" : "left", points: pts, moves: sent, pid: P.pid });
}
