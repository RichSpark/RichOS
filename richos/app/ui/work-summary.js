"use strict";
// Saved receipts are evidence of work, separate from the live worker count.
(function () {
  // **A BUSINESS OWNER READS THIS PANEL** (hunt 57, finding 11, 2026-10-15). Each row is the
  // title and its plain sentence (`work_status.rs`). The machinery line under it, the helper's
  // role and the repository ("consult · no repository"), is for Technical view only:
  // `opts.technical` is `main.js`'s `techyOn()`.
  function render(summary, parent, opts) {
    const technical = !!(opts && opts.technical);
    // A read that failed (hunt 57, finding 12) keeps the rows last read, and says why only in
    // Technical view: "Work status is changing" while Rich gets ready is nothing a business
    // owner needs to hear.
    if (summary.error && technical) {
      const note = document.createElement("p"); note.className = "overlay-note";
      note.textContent = "Saved work is unavailable: " + summary.error;
      parent.appendChild(note);
    }
    for (const item of summary.items || []) {
      const row = document.createElement("section"); row.className = "slide-item";
      const title = document.createElement("strong"); title.textContent = item.title;
      const detail = document.createElement("p"); detail.textContent = item.detail;
      row.append(title, detail);
      if (technical) {
        const repository = document.createElement("p"); repository.className = "overlay-note";
        repository.textContent = item.role + " · " + item.repository;
        repository.style.overflowWrap = "anywhere";
        row.append(repository);
      }
      parent.appendChild(row);
    }
    if (summary.omitted) {
      const note = document.createElement("p"); note.className = "overlay-note";
      note.textContent = `${summary.omitted} older team ${summary.omitted === 1 ? "job isn't" : "jobs aren't"} shown here. Ask Rich about any of them.`;
      parent.appendChild(note);
    }
  }
  // =====================================================================================
  // THE ASSIGNMENTS HE GAVE — background-work spec §1, §4.2 and §0 row 7
  // =====================================================================================
  //
  // A different thing from the receipts above, and the difference is the point: a receipt
  // describes what a worker did, and an assignment describes what the CEO ASKED FOR. It
  // exists from the moment he speaks, before a workspace does.
  //
  // **This is also the only surface that can show background work at all.** The live worker
  // chip is fed by `get_worker_status`, which returns an empty view whenever no turn is open
  // on the thread — and that window is where background work lives.
  //
  // WORDING IS THE FEATURE, not a detail of it — and what the words are FOR changed on
  // 2026-09-18 (CEO ruling §52): *"There's nothing that ever not lands on its own here in the
  // terminal. Anything including things like design mockups always land before they are
  // presented to me for review. So, yes, always land on its own."*
  //
  // `blocked` used to read "Ready for you to approve", because the one thing that could stop
  // a background job was the step that would change his repository — it was not on the
  // permission desk's list and the job waited there (spec §0 row 7, §5.4, §7.8). A job lands
  // on its own now, so:
  //
  //   * `blocked` means ONE thing: a decision of his is outstanding on some step — a command,
  //     a write outside the workspace — and Approve and Decline are beside it. It no longer
  //     claims his repository is untouched, because a job may reach such a step AFTER landing
  //     something, and that claim would have been false while his branch had already moved.
  //   * `settled` and `failed` carry the OUTCOME in the sentence the backend appends after
  //     them (`work_host.rs`'s `what_happened`, read off the land record the engine wrote):
  //     what landed, on which branch, in which repository, and the reviewer's verdict — or,
  //     for a job that did not land, why not.
  //
  // The §7.8 rule itself is untouched and still the floor: nothing that has not been witnessed
  // finishing is ever described as finished.
  //
  // CONTRAST, computed rather than eyeballed, both themes. Every figure below is WebKit's
  // own resolved color through `getComputedStyle`, taken by `ui/tests/background-work.js`
  // on 2026-09-18 — not a stylesheet value and not a recollection:
  //   heading    `--ink` on `--card`        12.06:1 dark   18.07:1 light   (needs 4.5:1)
  //   title      `--ink` on `--card`        12.06:1 dark   18.07:1 light   (needs 4.5:1)
  //   detail     `--ink-soft` on `--card`    5.78:1 dark    6.38:1 light   (needs 4.5:1)
  //   the question, 16px, same `--ink-soft`  5.78:1 dark    6.38:1 light   (needs 4.5:1)
  //   stop control, 16px, same `--ink-soft`  5.78:1 dark    6.38:1 light   (needs 4.5:1)
  //   approve and decline, 16px, the same    5.78:1 dark    6.38:1 light   (needs 4.5:1)
  // All seven at 16px, the floor for text meant to be easily read (`ceo-decisions.md` §15).
  // `--ink-soft` is `rgba(223,228,238,0.64)` over `#182440` and `rgba(12,19,34,0.68)` over
  // `#fdfcf8`; both were composited before the ratio was taken. Nothing here is exempt —
  // every ratio above is re-measured under WebKit by that suite in both themes rather than
  // trusted from this comment. (The light-mode soft figure reads 6.38:1 on this run where an
  // earlier comment recorded 6.36:1. Nothing in this file's palette, classes or opacity
  // changed; the measurement is the authority and the comment now carries what it measured.)
  const STATES = {
    registered: "Written down. Nothing has been prepared yet.",
    // Plain words for a business owner (hunt 57, finding 11): it was "Getting a workspace
    // ready.", and a workspace is machinery he never meets.
    preparing: "Getting ready to start.",
    running: "Running.",
    // A decision of his is outstanding, and the controls that make it are on the same row:
    // the desk holds his decision while he is away (§5.2/§5.7). The sentence names no
    // particular step — the "Waiting on you:" line below does that, in the backend's own
    // words for it — and it claims nothing about his repository (see the block above).
    blocked: "Waiting for your decision before it can go on.",
    // THE SAME STATE WITH NO QUESTION ON THE DESK, and it is a real case rather than a
    // defensive branch: the queue lives in the running process, so an assignment that
    // stopped at his decision before a relaunch is `blocked` with nothing left to press.
    // Naming a control that is not there would be the failure slice 1's affordance gate
    // caught; the composer is a real path and this sentence names it.
    blockedNoRequest: "Waiting for your decision. Ask Rich to continue it when you are ready.",
    // **WAITING FOR THE SCREEN — the CEO's ruling §56 (2026-09-18), in his own words.**
    //
    // Without this row the state word `waiting-for-screen` falls through to "Its state could
    // not be read." below, which is why it is here rather than in a follow-up: a new backend
    // state and this map are one change, and the fallback is what the CEO would have seen.
    //
    // **It names no control, and that is the point.** Every other waiting sentence in this map
    // either points at a control on the same row (`blocked`) or tells him what to ask for
    // (`blockedNoRequest`). This one asks nothing of him at all: it is waiting on the Mac, it
    // resolves itself, and the app must never tell him to go and unlock something. So it is
    // INFORMATIONAL for the affordance gate rather than actionable — there is no control that
    // could exist for it.
    //
    // CONTRAST: it renders in the same `detail` element as every other sentence in this map
    // (`.overlay-note`, `--ink-soft` on `--card`, 16px) — measured at the top of this file as
    // 5.78:1 dark and 6.38:1 light against a 4.5:1 floor. No new element, no new color, so the
    // floor is already cleared by those measurements rather than by a fresh claim.
    "waiting-for-screen": "Waiting for the screen to unlock. I'll carry on the moment it's back.",
    "waiting-for-quota": "Waiting for the allowance to refresh. I'll continue automatically when it's available.",
    // **The bare word, with the OUTCOME appended after it by the backend** (§52). The row
    // reads "Finished. It landed on cc/echo-1 in project. An independent review passed it
    // first." — and it still reads honestly on its own when an assignment closed with
    // nothing to land, which is why the outcome is not welded into this string.
    settled: "Finished.",
    // **A job that did not land lands HERE, and that is §52's third ending**, not only a
    // failed registration. The recorded reason follows it: the reviewer asked for changes,
    // the land did not go through, or nothing ran at all.
    failed: "Stopped before it finished. Ask Rich what it needs.",
    interrupted: "Stopped.",
  };

  // **A QUESTION IS NOT A JOB, AND THIS PANE MUST NOT CALL ONE FINISHED** — the CEO's ruling
  // §58, 2026-09-18: *"The answer arrives on the timeline as an answer, never as 'done'."*
  //
  // The timeline is where his answer goes, and that is where §58 is really about. But this
  // pane lists everything he has asked for, so a question he asked would have rendered here
  // under "Written down. Nothing has been prepared yet." and then "Finished." — the exact
  // framing the ruling refuses, on a surface he can open.
  //
  // Same three states, said about a question. `registered` and `preparing` are ONE sentence
  // here and two above, because "getting a workspace ready" describes a thing that is done
  // for work and is nothing he needs to hear about a question of his.
  const QUESTION_STATES = {
    registered: "Written down. Rich hasn't started looking yet.",
    preparing: "Written down. Rich hasn't started looking yet.",
    running: "Rich is looking into this.",
    blocked: "Waiting for your decision before Rich can go on.",
    // The same stop with nothing left on the desk: the queue lives in the running process, so
    // a question that stopped at his decision before a relaunch has nothing to press. **This
    // line was dropped by the §56 rebase and the affordance gate is what found it** — without
    // it the state falls through to "Its state could not be read.", which is the honest
    // sentence for a record this build cannot parse and a false one for a record it can.
    blockedNoRequest: "Waiting for your decision. Ask Rich to continue when you are ready.",
    // **§56's state, in a question's words.** Without this row a question waiting on the
    // screen falls through to "Its state could not be read." — the same gap §56's own slice
    // closed for work, arriving one commit later for questions.
    "waiting-for-screen": "Waiting for the screen to unlock. Rich will carry on the moment it's back.",
    "waiting-for-quota": "Waiting for the allowance to refresh. Rich will continue automatically when it's available.",
    // NOT "Finished." — the answer itself is on the conversation, which is where he reads it.
    settled: "Answered: it's in your conversation.",
    failed: "Rich couldn't get you an answer. Ask him again and he'll try it a different way.",
    interrupted: "Stopped.",
  };

  /// Which set of sentences this row gets. `kind` comes from the assignment register
  /// (`get_assignments`); anything that is not one of the two question kinds is work, which
  /// is what every record written before 2026-09-18 is.
  ///
  /// **`answered` wins over the kind** (esc-20260927T093052Z-85f3303f). A task the back end
  /// did itself, with no helper, is closed by its report on his timeline — the command's
  /// result, or what it needs before it can begin — and "Finished." would be a claim nobody
  /// witnessed. So it gets the sentence an answered question gets: it's in your conversation.
  function sentencesFor(kind, answered) {
    return answered || kind === "check" || kind === "investigate" ? QUESTION_STATES : STATES;
  }

  /// `onDecide(requestId, allow)` answers the ONE exact request this assignment is waiting
  /// on him for. It is a request id and not an assignment id on purpose: what he approves is
  /// one action, and the press has to carry which one (spec §5.2's *"one exact action"*
  /// survives the queue).
  ///
  /// `opts.technical` is `main.js`'s `techyOn()`, the same switch `render` takes above: the
  /// back end's recorded line under a row is for Technical view only (hunt 57, finding 11).
  function renderAssignments(view, parent, onStop, onDecide, opts) {
    const technical = !!(opts && opts.technical);
    if (view.error) {
      const note = document.createElement("p");
      note.className = "overlay-note";
      note.textContent = "Your assignments are unavailable: " + view.error;
      parent.appendChild(note);
      return;
    }
    const rows = view.rows || [];
    if (!rows.length) return;
    const heading = document.createElement("h3");
    heading.className = "assignment-heading";
    heading.textContent = "What you have asked for";
    parent.appendChild(heading);
    for (const row of rows) {
      const item = document.createElement("section");
      item.className = "slide-item";
      const title = document.createElement("strong");
      title.className = "assignment-title";
      title.textContent = row.title;
      const detail = document.createElement("p");
      detail.className = "overlay-note";
      // The state's own sentence, then whatever the backend recorded about it. Two plain
      // sentences rather than a code and a label.
      const waiting = row.awaitingYou || null;
      const key = row.state === "blocked" && !waiting ? "blockedNoRequest" : row.state;
      detail.textContent = sentencesFor(row.kind, row.answered)[key] || "Its state could not be read.";
      // The recorded detail follows the state's sentence, because for most states it ADDS
      // something — the outcome that landed, the reason it stopped, the step it is on.
      //
      // **Two states where it would not, and they arrived on the same day.**
      // `waiting-for-screen` (§56): the record's own detail is the same fact as the sentence
      // above it in other words, and the status read, which shows only the detail, needs it
      // to be a whole sentence — so the record keeps it and this row drops it rather than
      // telling him the same thing twice. And a QUESTION (§58): its detail is a record's
      // internal note ("Answered.", "Opening the work connection.") that says nothing he
      // needs and reads as machinery beside a sentence written for him.
      //
      // **AND IN TECHNICAL VIEW ONLY, ON ITS OWN LINE** (hunt 57, finding 11, the same rule as
      // the saved-work rows above). What the back end records is written for whoever reads the
      // machinery: "Opening the work connection.", "The back end couldn't take it yet", "It
      // landed on cc/echo-1 in project.", or an error relayed as it came. A business owner reads
      // the state's own sentence, which is complete on its own.
      const addsSomething = row.state !== "waiting-for-screen" && sentencesFor(row.kind, row.answered) === STATES;
      item.append(title, detail);
      if (technical && row.detail && addsSomething) {
        const recorded = document.createElement("p");
        recorded.className = "overlay-note";
        recorded.textContent = row.detail;
        recorded.style.overflowWrap = "anywhere";
        item.append(recorded);
      }
      if (waiting) {
        // WHAT HE IS BEING ASKED, in the backend's own words for it — one phrase, written
        // in one place (`work_host.rs`'s `plain_action`), so the receipt and this row can
        // never describe one step two ways.
        const asked = document.createElement("p");
        asked.className = "overlay-note assignment-asked";
        asked.textContent = "Waiting on you: " + (waiting.asked || "a step it cannot take without you") + ".";
        const decisions = document.createElement("div");
        decisions.className = "assignment-decisions";
        // **Each control names its own assignment.** The same reason the stop control does:
        // a button labeled only "Approve" beside three assignments is one he cannot use
        // without counting rows, and spoken aloud it would name nothing at all.
        const approve = document.createElement("button");
        approve.type = "button";
        approve.className = "slideover-close assignment-approve";
        approve.textContent = "Approve this";
        approve.setAttribute("aria-label", "Approve " + row.title);
        approve.addEventListener("click", () => onDecide && onDecide(waiting.requestId, true));
        const decline = document.createElement("button");
        decline.type = "button";
        decline.className = "slideover-close assignment-decline";
        decline.textContent = "Decline this";
        decline.setAttribute("aria-label", "Decline " + row.title);
        decline.addEventListener("click", () => onDecide && onDecide(waiting.requestId, false));
        decisions.append(approve, decline);
        item.append(asked, decisions);
      }
      if (row.canStop) {
        const stop = document.createElement("button");
        stop.type = "button";
        stop.className = "slideover-close assignment-stop";
        // **Per assignment, and it says which one.** Stop stays the conversation's Stop
        // (the CEO, 2026-09-17: "OK, go with recommended"), so this control is the only way
        // to stop one piece of running work, and a control labeled just "Stop" beside three
        // of them would be a control he cannot use without counting rows.
        stop.textContent = "Stop this";
        stop.setAttribute("aria-label", "Stop " + row.title);
        stop.addEventListener("click", () => onStop(row.id));
        item.appendChild(stop);
      }
      parent.appendChild(item);
    }
  }

  window.RichWorkSummary = {render, renderAssignments};
})();
