// THE OXFORD COMMA RULE (CEO, 2026-10-19): ", and" becomes " and", ", or" becomes " or" and
// ", nor" becomes " nor". Code is skipped ("Obviously the Oxfort comma linter can't be allowed to
// mess with code."): fenced code blocks, inline code and HTML <pre>/<code>.
//
// One file, three users: the display filter for what Claude writes (desktop and phone page, while
// the setting is off), the commit-time fixer (`app/scripts/oxford-comma.js`) and the check in
// `ui/tests/dialect.js`. The phone page gets this same file through `src-tauri/build.rs`.
(function (root) {
  "use strict";

  var CODE = /```[\s\S]*?(?:```|$)|~~~[\s\S]*?(?:~~~|$)|<(pre|code)\b[\s\S]*?(?:<\/\1>|$)|`[^`\n]*`/g;
  var COMMA = /,( (?:and|or|nor)\b)/g;

  function strip(text) {
    text = String(text == null ? "" : text);
    var out = "";
    var last = 0;
    var m;
    CODE.lastIndex = 0;
    while ((m = CODE.exec(text)) !== null) {
      out += text.slice(last, m.index).replace(COMMA, "$1") + m[0];
      last = CODE.lastIndex;
    }
    return out + text.slice(last).replace(COMMA, "$1");
  }

  // The setting on this page: `true` keeps the comma. Default off, his words.
  var keep = false;
  var listeners = [];

  var api = {
    strip: strip,
    display: function (text) {
      return keep ? String(text == null ? "" : text) : strip(text);
    },
    keeping: function () {
      return keep;
    },
    setKeeping: function (on) {
      if (!!on === keep) return false;
      keep = !!on;
      listeners.forEach(function (fn) { fn(keep); });
      return true;
    },
    onChange: function (fn) {
      listeners.push(fn);
    },
  };

  if (typeof module === "object" && module.exports) module.exports = api;
  if (root) root.RichOxford = api;
})(typeof self !== "undefined" ? self : typeof window !== "undefined" ? window : null);
