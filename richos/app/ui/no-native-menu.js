// THE WEB VIEW'S OWN RIGHT-CLICK MENU NEVER APPEARS (CEO, nightly 65, 2026-10-19: Look Up,
// Translate, Search with Google, Share, Speech, Services on the company name — "When will all
// shit of this kind be gone from the right-click events?").
//
// ONE PLACE FOR THE WHOLE WINDOW, loaded first by every document the app opens (index.html and
// the two dictation windows), so no element and no panel has to remember to suppress it.
//
//  * A context click shows nothing, except in a field the user types in (input, textarea,
//    contenteditable), where the plain editing menu stays so Cut, Copy and Paste still work.
//  * Wherever the app has its own right-click menu (thread rows, files), its handler still runs:
//    this listener only cancels the default and never stops propagation.
//  * A right-click does not select the text under it (WebKit selects the word on mouse-down).
(function () {
  "use strict";
  var TYPED_INTO = 'textarea, [contenteditable=""], [contenteditable="true"], [contenteditable="plaintext-only"], ' +
    'input:not([type]), input[type="text"], input[type="search"], input[type="url"], input[type="tel"], ' +
    'input[type="email"], input[type="password"], input[type="number"]';
  function inField(target) {
    return target instanceof Element && target.closest(TYPED_INTO) !== null;
  }
  var mac = /Mac/.test(navigator.platform || "");
  // WebKit selects the word under a context click, at mouse-down on some builds and when the
  // context menu is built on others; so the selection the user had is put back after both.
  var kept = null;
  function keep() {
    var s = window.getSelection();
    kept = [];
    for (var i = 0; i < s.rangeCount; i++) kept.push(s.getRangeAt(i).cloneRange());
  }
  function restore() {
    if (!kept) return;
    var s = window.getSelection();
    s.removeAllRanges();
    kept.forEach(function (r) { s.addRange(r); });
  }
  window.addEventListener("mousedown", function (e) {
    var contextClick = e.button === 2 || (mac && e.button === 0 && e.ctrlKey);
    if (!contextClick || inField(e.target)) return;
    keep();
    e.preventDefault();
  }, true);
  window.addEventListener("contextmenu", function (e) {
    if (inField(e.target)) return;
    e.preventDefault();
    restore();
    requestAnimationFrame(function () { restore(); kept = null; });
  }, true);
})();
