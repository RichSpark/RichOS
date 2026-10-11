"use strict";
// =======================================================================================
// "YOU HAVE WORK RUNNING. QUIT ANYWAY?" — the background-work spec §2.5, §2.5a, §7.4a
// =======================================================================================
//
// The CEO's row 5: *"You close the window. The work keeps going. Quit ends it."* Closing
// the window no longer quits, so this sheet is rare rather than constant — it appears only
// when he chooses Quit while something is actually running or waiting for him.
//
// **The decision is already made by the time this renders.** §2.5a: the runtime reads the
// exit answer synchronously, so the shell PREVENTS the quit first and asks second. Pressing
// "Keep working" therefore does nothing at all — the work was never touched — and that is
// why this file has no undo, no retry and no state of its own.
//
// **The sentence comes from the shell** (`src-tauri/src/lifecycle.rs`'s `quit_question`),
// in his own terms, and so do both control labels. One place, so what the button says and
// what it does cannot drift apart.
//
// CONTRAST, COMPUTED AND NOT EYEBALLED, both themes — the standing floor (`CLAUDE.md`).
// Every class here is one the permission sheet already uses, and every ratio is re-measured
// under WebKit by `ui/tests/quit-question.js` rather than trusted from this comment:
//   title      `--ink` on `--card`             12.06:1 dark   18.07:1 light  (needs 4.5:1)
//   question   `--ink-soft` on `--card`         5.78:1 dark    6.36:1 light  (needs 4.5:1)
//   both controls, `.desk-btn`, 16px            measured in the suite, both themes
// Nothing here is declared exempt: a question he is being asked is the definition of text
// meant to be read.
(function () {
  const panel = document.createElement("div");
  panel.id = "quit-question";
  panel.className = "overlay";
  panel.hidden = true;
  panel.setAttribute("role", "dialog");
  panel.setAttribute("aria-labelledby", "quit-question-title");
  // ESCAPE KEEPS WORKING, the same way the permission sheet's Escape declines: the safe
  // answer is the one that changes nothing.
  panel.setAttribute("data-dismiss", "control:#quit-question-stay");
  panel.innerHTML = `<div class="overlay-panel overlay-panel--compact">
    <h2 id="quit-question-title" class="overlay-title">Quit while work is running?</h2>
    <p id="quit-question-say" class="overlay-note"></p>
    <div class="desk-card-actions">
      <button id="quit-question-stay" class="desk-btn" type="button">Keep working</button>
      <button id="quit-question-quit" class="desk-btn desk-btn--confirm" type="button">Quit and stop the work</button>
    </div>
  </div>`;
  document.body.appendChild(panel);

  const say = panel.querySelector("#quit-question-say");
  const stay = panel.querySelector("#quit-question-stay");
  const quit = panel.querySelector("#quit-question-quit");
  let returnFocus = null;

  /// Put the question up. `payload` is the shell's: `{say, quit, stay}`.
  function show(payload) {
    say.textContent = (payload && payload.say) || "Something is still running in the background.";
    // The labels are the shell's words too. A default is kept for the case where the event
    // arrives without them, because a button with no label is worse than a plain one.
    stay.textContent = (payload && payload.stay) || "Keep working";
    quit.textContent = (payload && payload.quit) || "Quit and stop the work";
    returnFocus = document.activeElement;
    panel.hidden = false;
    // THE SAFE ANSWER TAKES FOCUS. A sheet that opened with "quit and stop the work" under
    // the return key would answer itself.
    stay.focus();
  }

  function hide() {
    panel.hidden = true;
    if (returnFocus && returnFocus.focus) returnFocus.focus();
    returnFocus = null;
  }

  async function answer(quitting) {
    stay.disabled = true;
    quit.disabled = true;
    try {
      await window.RichBridge.invoke(quitting ? "confirm_quit_and_stop" : "cancel_quit", {});
      // On the quitting path the process is going away and this line may never run; on the
      // other one the sheet closes and the work carries on untouched.
      if (!quitting) hide();
    } catch (error) {
      // The refusal is said rather than swallowed: he pressed something and must not be
      // left looking at a sheet that did nothing.
      say.textContent = "That could not be done: " + String(error);
    } finally {
      stay.disabled = false;
      quit.disabled = false;
    }
  }

  stay.addEventListener("click", () => answer(false));
  quit.addEventListener("click", () => answer(true));
  panel.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      answer(false);
    }
    if (event.key === "Tab") {
      event.preventDefault();
      (document.activeElement === stay ? quit : stay).focus();
    }
  });

  if (window.RichBridge && window.RichBridge.listen) {
    window.RichBridge.listen("rich://quit-question", ({ payload }) => show(payload));
  }

  // Exported for the suite, which drives the real renderer rather than a copy of it.
  window.RichQuitQuestion = { show, hide, panel };
})();
