/* Numeric authorship for editable drafts containing app-generated attachment context. */
window.RichAuthored = (function () {
  "use strict";
  const encoder = new TextEncoder();
  const length = text => encoder.encode(text).length;
  function normalized(text, ranges) {
    if (!Array.isArray(ranges) || ranges.some(pair => !Array.isArray(pair) || pair.length !== 2)) return null;
    const bytes = encoder.encode(text), result = [];
    for (const pair of [...ranges].sort((a, b) => a[0] - b[0])) {
      if (!Array.isArray(pair) || pair.length !== 2) return null;
      const [start, end] = pair;
      if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 0 || end < start || end > bytes.length) return null;
      if ((start < bytes.length && (bytes[start] & 192) === 128) || (end < bytes.length && (bytes[end] & 192) === 128)) return null;
      if (start === end) continue;
      const last = result[result.length - 1];
      if (last && start <= last[1]) last[1] = Math.max(last[1], end);
      else result.push([start, end]);
    }
    return result;
  }
  function ranges(text, capture) {
    if (capture && Array.isArray(capture.authoredRanges)) return normalized(text, capture.authoredRanges);
    const end = capture && capture.authoredBytes;
    return Number.isSafeInteger(end) && end >= 0 ? normalized(text, [[0, end]]) : null;
  }
  function capture(text, owned) {
    const clean = normalized(text, owned);
    if (!clean) { const outside = length(text) + 1; return { authoredRanges: [[outside, outside]] }; }
    if (!clean.length) return { authoredBytes: 0 };
    return clean.length === 1 && clean[0][0] === 0 ? { authoredBytes: clean[0][1] } : { authoredRanges: clean };
  }
  function forSend(saved, text) {
    if (!saved) return { authoredBytes: length(text) };
    if (saved.text === text) return capture(text, saved.ranges);
    if (saved.text.trim() === text) {
      const offset = length(saved.text) - length(saved.text.trimStart()), end = offset + length(text);
      const owned = saved.ranges && saved.ranges.map(([a, b]) => [Math.max(a, offset), Math.min(b, end)])
        .filter(([a, b]) => b > a).map(([a, b]) => [a - offset, b - offset]);
      return capture(text, owned);
    }
    return capture(text, edit(saved, text).ranges);
  }
  const equal = (a, b) => a.length === b.length && a.every((value, i) => value === b[i]);
  function edit(saved, text, hint) {
    const old = encoder.encode(saved.text), next = encoder.encode(text);
    let start = 0, end = old.length, inserted = next.length;
    const hintLength = hint && next.length - (old.length - (hint.end - hint.start));
    if (hint && hint.start >= 0 && hint.end >= hint.start && hint.end <= old.length && hintLength >= 0
      && equal(old.subarray(0, hint.start), next.subarray(0, hint.start))
      && equal(old.subarray(hint.end), next.subarray(hint.start + hintLength))) {
      start = hint.start; end = hint.end; inserted = hintLength;
    } else {
      while (start < old.length && start < next.length && old[start] === next[start]) start++;
      while (start > 0 && ((old[start] & 192) === 128 || (next[start] & 192) === 128)) start--;
      let suffix = 0;
      while (suffix < old.length - start && suffix < next.length - start && old[old.length - suffix - 1] === next[next.length - suffix - 1]) suffix++;
      while (suffix > 0 && ((old[old.length - suffix] & 192) === 128 || (next[next.length - suffix] & 192) === 128)) suffix--;
      end = old.length - suffix; inserted = next.length - suffix - start;
    }
    if (!saved.ranges && !(start === 0 && end === old.length)) return { text, ranges: null };
    const owned = [], shift = inserted - (end - start);
    for (const [a, b] of saved.ranges || []) {
      if (a < start) owned.push([a, Math.min(b, start)]);
      if (b > end) owned.push([Math.max(a, end) + shift, b + shift]);
    }
    if (inserted > 0) owned.push([start, start + inserted]);
    return { text, ranges: normalized(text, owned) };
  }
  function join(text, leftCapture, draft, rightCapture) {
    if (!draft.trim() || draft === text) return { text, ranges: ranges(text, leftCapture) };
    const combined = text + "\n\n" + draft, offset = length(text + "\n\n");
    const left = ranges(text, leftCapture), right = ranges(draft, rightCapture);
    return { text: combined, ranges: left && right ? normalized(combined, [...left, ...right.map(([a, b]) => [a + offset, b + offset])]) : null };
  }
  return { length, normalized, ranges, capture, forSend, edit, join };
})();
