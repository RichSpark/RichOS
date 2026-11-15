// NOTHING A SUITE RUNS MAKES A SOUND (CEO ruling 53; and 2026-11-14: a mockup walk played its
// test videos aloud). The splash screen has a sound since round 74, and a headless browser
// still plays what a page plays. A suite that can reach it (the splash with the sound on, the
// switch that turns it on) installs this before the page's own scripts, or evaluates it before
// the press: `Audio` becomes a recorder that plays nothing, and what would have played, and at
// which moment of the splash, is read from `window.__audio`.
"use strict";

const SILENT_AUDIO = () => {
  window.__audio = window.__audio || [];
  window.Audio = function (src) {
    const a = {
      src: String(src),
      preload: "",
      loaded: false,
      playedAt: null,
      load() { a.loaded = true; },
      play() {
        a.playedAt = window.RichSplash && window.RichSplash.moment() !== null ? Math.round(window.RichSplash.moment()) : -1;
        return Promise.resolve();
      },
    };
    window.__audio.push(a);
    return a;
  };
};

module.exports = { SILENT_AUDIO };
