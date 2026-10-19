// THE OXFORD COMMA FILTER (CEO, 2026-10-19). His rule, word for word: "This linter should just
// automatically delete a comma if it comes before ` and` or ` or` or ` nor`."
//
// ONE FILTER, THREE USERS. The same file is:
//
//   1. the display filter for what Claude writes (Rich's and Stu's replies in this window, the
//      questions Claude writes on question cards, and the replies on the phone page), applied
//      where the text is drawn while the "Oxford comma" setting is off. Stored messages are
//      never rewritten; `Copy` still copies what was written.
//   2. the core of the commit-time fixer (`app/scripts/oxford-comma.js`), which cleans the app's
//      own UI text and every Markdown doc a commit adds or changes.
//   3. the core of the check beside the m-dash one in `ui/tests/dialect.js`.
//
// The phone page gets this exact file: `src-tauri/build.rs` stages it into the phone app as
// `lib/oxford-comma.js`, so there is no second copy of the rule to drift.
//
// WHAT IS NEVER TOUCHED. Code is not prose: a fenced code block and an inline code span keep
// every comma. With `quoted: true` (the app's own text and the docs, where quoted material is
// someone's exact words) a passage in double quotes, a blockquote line and an HTML comment are
// left alone too. With `doc: true` (a Markdown file) YAML front matter and an indented code
// block are left alone as well.
//
// THE MATCH. A comma, then spaces or tabs (one line break allowed, for wrapped Markdown), then
// the lowercase word `and`, `or` or `nor` ending at a word boundary. "Android" and "order" do
// not match; neither does "AND" in an all-caps heading.
(function (root) {
  "use strict";

  var AFTER = /^(?:[ \t ]+|[ \t ]*\r?\n[ \t ]*)(?:and|or|nor)(?![A-Za-z0-9_])/;
  var FENCE = /^[ \t]{0,3}(`{3,}|~{3,})/;

  function lineStarts(text) {
    var starts = [0];
    for (var i = 0; i < text.length; i++) if (text[i] === "\n") starts.push(i + 1);
    return starts;
  }

  /// The ranges `[from, to)` of `text` that keep their commas.
  function protectedRanges(text, opts) {
    var out = [];
    var starts = lineStarts(text);
    var lines = text.split("\n");
    var codeLine = new Array(lines.length);

    // Front matter: a `---` first line through the next `---` line.
    var first = 0;
    if (opts.doc && /^---[ \t]*\r?$/.test(lines[0] || "")) {
      for (var f = 1; f < lines.length; f++) {
        if (/^(---|\.\.\.)[ \t]*\r?$/.test(lines[f])) {
          for (var g = 0; g <= f; g++) codeLine[g] = true;
          first = f + 1;
          break;
        }
      }
    }

    // Fenced code blocks. An unclosed fence runs to the end (CommonMark), so a comma inside a
    // half-written block is never mistaken for prose.
    var open = null;
    for (var i = first; i < lines.length; i++) {
      var m = FENCE.exec(lines[i]);
      if (open) {
        codeLine[i] = true;
        if (m && m[1][0] === open[0] && m[1].length >= open.length && !/[^ \t\r`~]/.test(lines[i].replace(FENCE, ""))) open = null;
        continue;
      }
      if (m) {
        open = m[1];
        codeLine[i] = true;
      }
    }

    // Indented code blocks (docs only): a line indented four spaces or a tab, after a blank line,
    // and the indented lines that follow it. Conservative on purpose: a list's indented
    // continuation is left alone too, because a comma kept is harmless and a comma deleted from
    // code is not.
    if (opts.doc) {
      var inIndented = false;
      for (var k = first; k < lines.length; k++) {
        if (codeLine[k]) { inIndented = false; continue; }
        var blank = !lines[k].trim();
        var indented = /^( {4}|\t)/.test(lines[k]);
        if (indented && (inIndented || k === 0 || !lines[k - 1].trim())) {
          inIndented = true;
          codeLine[k] = true;
        } else if (!blank) {
          inIndented = false;
        }
      }
    }

    for (var c = 0; c < lines.length; c++) {
      if (codeLine[c]) out.push([starts[c], starts[c] + lines[c].length]);
      else if (opts.quoted && /^[ \t]{0,3}>/.test(lines[c])) out.push([starts[c], starts[c] + lines[c].length]);
    }
    var isCode = function (idx) {
      for (var r = 0; r < out.length; r++) if (idx >= out[r][0] && idx < out[r][1]) return true;
      return false;
    };

    // Inline code: a run of N backticks closed by a run of exactly N, within one paragraph. An
    // unmatched run is literal and protects nothing.
    var p = 0;
    while (p < text.length) {
      if (text[p] !== "`" || isCode(p)) { p++; continue; }
      var n = 0;
      while (text[p + n] === "`") n++;
      var q = p + n;
      var closed = -1;
      while (q < text.length) {
        if (text[q] === "\n" && text[q + 1] === "\n") break;
        if (text[q] === "`") {
          var len = 0;
          while (text[q + len] === "`") len++;
          if (len === n) { closed = q; break; }
          q += len;
          continue;
        }
        q++;
      }
      if (closed < 0) { p += n; continue; }
      out.push([p, closed + n]);
      p = closed + n;
    }

    if (opts.quoted) {
      // HTML comments.
      var re = /<!--[\s\S]*?(?:-->|$)/g;
      var h;
      while ((h = re.exec(text)) !== null) {
        out.push([h.index, h.index + h[0].length]);
        if (h[0].length === 0) re.lastIndex++;
      }
      // Quotations: curly pairs, and straight pairs on one line. An unpaired mark is literal.
      for (var l = 0; l < lines.length; l++) {
        var s = lines[l];
        var base = starts[l];
        var openAt = -1;
        for (var j = 0; j < s.length; j++) {
          if (s[j] !== '"') continue;
          if (isCode(base + j)) continue;
          if (openAt < 0) openAt = j;
          else { out.push([base + openAt, base + j + 1]); openAt = -1; }
        }
      }
      var curly = /“[^”]*”/g;
      var cm;
      while ((cm = curly.exec(text)) !== null) out.push([cm.index, cm.index + cm[0].length]);
    }
    return out;
  }

  /// The index of every comma in `text` this rule deletes, ascending.
  function commaOffsets(text, opts) {
    text = String(text == null ? "" : text);
    opts = opts || {};
    if (text.indexOf(",") < 0) return [];
    var ranges = null;
    var out = [];
    for (var i = text.indexOf(","); i >= 0; i = text.indexOf(",", i + 1)) {
      if (!AFTER.test(text.slice(i + 1, i + 40))) continue;
      if (!ranges) ranges = protectedRanges(text, opts);
      var kept = false;
      for (var r = 0; r < ranges.length; r++) {
        if (i >= ranges[r][0] && i < ranges[r][1]) { kept = true; break; }
      }
      if (!kept) out.push(i);
    }
    return out;
  }

  /// `text` with every such comma deleted.
  function strip(text, opts) {
    text = String(text == null ? "" : text);
    var at = commaOffsets(text, opts);
    if (!at.length) return text;
    var parts = [];
    var from = 0;
    for (var i = 0; i < at.length; i++) {
      parts.push(text.slice(from, at[i]));
      from = at[i] + 1;
    }
    parts.push(text.slice(from));
    return parts.join("");
  }

  // THE SETTING, ON THIS PAGE. `true` means the user keeps the Oxford comma (nothing is
  // deleted); the default is off, his words: "Default setting: Oxford comma off." The durable
  // value lives in the app's config (desktop) or comes from the Mac (phone); this is only the
  // copy the drawing code reads, with listeners so a switch takes effect at once.
  var keep = false;
  var listeners = [];

  var api = {
    commaOffsets: commaOffsets,
    strip: strip,
    /** What Claude wrote, as it is drawn: unchanged while the setting is on, the comma deleted
     *  while it is off. Code is never touched. */
    display: function (text) {
      return keep ? String(text == null ? "" : text) : strip(text);
    },
    keeping: function () {
      return keep;
    },
    setKeeping: function (on) {
      var next = !!on;
      if (next === keep) return false;
      keep = next;
      for (var i = 0; i < listeners.length; i++) {
        try { listeners[i](keep); } catch (e) { /* one bad listener never stops the others */ }
      }
      return true;
    },
    onChange: function (fn) {
      if (typeof fn === "function") listeners.push(fn);
    },
  };

  if (typeof module === "object" && module.exports) module.exports = api;
  if (root) root.RichOxford = api;
})(typeof self !== "undefined" ? self : typeof window !== "undefined" ? window : null);
