// RichOS web UI — the Codex-inspired working timeline (UX brief §5, §6, §7, §15).
//
// This file owns TWO things and nothing else:
//
//   1. THE MODEL — a client-side projection of the typed timeline (§12). It is fed from
//      exactly two sources, and they agree by construction because they carry the same
//      item ids: the `get_timeline` snapshot (the reload path) and the six additive
//      `rich://` events (the live path, app/STREAMING.md "The additive live-work family").
//   2. THE RENDER — the CEO bubble (§5.1), Rich's prose (§5.2/§5.4), semantic work
//      activity (§5.3), delegated AI workers and their chips (§7.1), the read-only worker
//      inspector's BODY (§7.2 — the pane's shell and its divider are main.js's), the
//      working-duration row (§6) and its collapse (§6.4).
//
// It talks to no bridge, listens to no event and invokes no command. `main.js` drives it.
// That split is deliberate: every rule below is testable in a browser with no Tauri, no
// engine and no lease.
//
// =========================================================================================
// THE ONE THING TO READ BEFORE CHANGING ANYTHING HERE: `phase` IS UNKNOWN.
// =========================================================================================
//
// §5.2 (commentary) and §5.4 (the executive response) ask for two different treatments —
// commentary folds into the collapsible working transcript, the final response stays
// expanded outside it. **The data cannot tell them apart today, and this file does not
// pretend otherwise.**
//
// Measured, not assumed (`docs/verification/acp-emission-probe-2026-08-28.md` §2, five
// runs): 52 `agent_message_chunk`s and ZERO message-open, message-close or role updates.
// Nothing on the wire separates Rich thinking out loud from Rich answering — not the ACP
// adapter's, and not the native binary's, whose `message_start`/`message_stop` bracket a
// message and say nothing about what KIND of message it is. And
// `rich://message-started` fires when the first delta is persisted — before the turn ends —
// so even a perfect after-the-fact rule would not be available at emission time.
// `richos_core::live::STREAMED_MESSAGE_PHASE` is a named constant equal to `Unknown`
// precisely so nobody writes `phase ?? "final"`.
//
// So this renderer takes the one honest option:
//
//   * EVERY run of Rich's prose renders the same way, in sequence order — one lane, one
//     treatment. No "final answer" styling, no larger type for the last run, no citation
//     chrome, nothing that says "this is the deliverable".
//   * **The collapse (§6.4) never hides prose.** Collapsing "the commentary" when
//     commentary cannot be distinguished from the answer would put the CEO's deliverable
//     behind a chevron. So the disclosure collapses exactly what IS positively typed as
//     work — the semantic activity rows — and prose stays visible at every state.
//   * The tempting fallback is also false: *"the last run of a completed turn is the final
//     answer"* breaks the moment Rich verifies something after writing his conclusion,
//     which makes the last run a two-word "Confirmed." and the deliverable the run before
//     it.
//
// The ONE phase that is real is `proactive` — the ledger records it — and it keeps its
// existing "reached out" treatment.
//
// When a real phase signal lands, the change is local: `isProse()` stays, and
// `renderTurn()` gains a split between the collapsible lane and the response lane.
"use strict";

(function () {
  // -------------------------------------------------------------------------------------
  // §6.2 DURATION FORMAT
  // -------------------------------------------------------------------------------------

  /// §6.2's table, verbatim, with the arithmetic shown:
  ///
  ///   | elapsed            | rendered      |
  ///   | 0 to 999ms         | (none)        |  -> null, NOT "0s"
  ///   | 1s to 59s          | `18s`         |  18_360ms -> floor(18.36) -> "18s"
  ///   | 1m to 59m 59s      | `4m 7s`       |  247_000ms -> 4m + 7s
  ///   | 1h to 23h 59m 59s  | `2h 17m 50s`  |  8_270_000ms -> 2h + 17m + 50s
  ///   | 24h or more        | `1d 3h`       |  97_200_000ms -> 27h -> 1d + 3h
  ///
  /// Space-separated, never zero-padded (§6.2). Truncation, not rounding: a turn that ran
  /// 18.9s reads `18s`, because rounding UP would report time that had not yet elapsed.
  function formatDuration(ms) {
    if (typeof ms !== "number" || !isFinite(ms) || ms < 0) return null;
    if (ms < 1000) return null; // "no duration yet" — an explicit row in §6.2's table
    const totalSec = Math.floor(ms / 1000);
    const s = totalSec % 60;
    const totalMin = Math.floor(totalSec / 60);
    const m = totalMin % 60;
    const totalHour = Math.floor(totalMin / 60);
    const h = totalHour % 24;
    const d = Math.floor(totalHour / 24);
    if (d >= 1) return `${d}d ${h}h`;
    if (totalHour >= 1) return `${h}h ${m}m ${s}s`;
    if (totalMin >= 1) return `${m}m ${s}s`;
    return `${s}s`;
  }

  // -------------------------------------------------------------------------------------
  // §6.1 THE LABELS — and the two §6.1 labels this build refuses to draw
  // -------------------------------------------------------------------------------------
  //
  // §6.1 lists six labels. Four are drawn. Two are NOT, and their absence is sourced:
  //
  //   `You stopped after {duration}`  — needs a CEO-stop signal. §9.3's stop control does
  //       not exist (slice 6), and `WorkState::Interrupted` covers a crash, a rotation and
  //       a cancel alike (timeline.rs). Rendering it would attribute the stop to the CEO on
  //       no evidence, so an interrupted turn reads `Stopped after {duration}` instead —
  //       §6.1's *failed* label, which claims only what is recorded.
  //   `Waiting for you`              — §11's `waiting_for_user` does not exist in this
  //       runtime (live.rs), so nothing can put a turn in it.
  //
  // And one label that is NOT in §6.1 at all, added because §14 demands it: a turn that is
  // `working` on disk with no live turn running for it in this session. §14: *"Never infer
  // that a turn completed because the app was closed … Show `Reconnecting` or `Status
  // unavailable` honestly."* Ticking a timer from its `startedAt` at read time is the exact
  // twelve-hour trap §6.3 names, so no duration is offered at all.

  /// WHAT THE DURATION MEANS — and why the §6.1 wording is not an overclaim TODAY.
  ///
  /// §6.3: the timer measures ACTIVE turn time; it pauses in `waiting_for_user`. This build
  /// has no pause, so there is no interval that could be excluded:
  /// `ledger.rs` `Turn::active_ms` — *"There is no pause accounting because there is no
  /// pause: §11's `waiting_for_user` state does not exist in this runtime yet, so no
  /// interval can be excluded. When it lands, this measure becomes wall time and MUST be
  /// replaced by accumulated active time, not extended."*
  ///
  /// So `active_ms` = `ended_at - started_at` = active time = wall time, and the three are
  /// equal BY CONSTRUCTION rather than by assumption. `Worked for 4m 7s` is exact.
  ///
  /// **The day `waiting_for_user` lands, this stops being true and this label becomes a
  /// lie** — it would then bill the CEO's lunch break to Rich. Whoever lands that state
  /// must replace `active_ms` with accumulated active time (ledger.rs says so) BEFORE this
  /// row ships against it.
  const DURATION_MEANING =
    "Active time from when Rich accepted the message to when the turn ended.";

  /// Said while the stop is in flight. It claims only what is true at that instant: the
  /// request is recorded, and Rich has been told. Whether the lease has actually let go
  /// yet is not knowable from here, so it is not asserted.
  const STOP_MEANING = "Your stop is recorded. Rich is letting go of this turn.";

  /// Turn -> the row's label, its number, its tone and its accessible description.
  ///
  /// `t`: { status, startedAt, activeMs, live, mergedFrom }
  /// `nowMs`: the clock for a LIVE tick only. Never consulted for a terminal turn.
  /// **THE SIXTY SECONDS, AND THE ONE PLACE THEY EXIST IN THIS BUILD** — the CEO's ruling
  /// §58 (`richos-hq` `wiki/ceo-decisions.md:2884-2914`, 2026-09-18): *"a check still running
  /// at one minute flips its label to 'investigating' on its own."*
  ///
  /// The register does NOT rewrite the kind when this fires. The record keeps what the front
  /// desk estimated, which is the only thing that can later be compared against what actually
  /// happened; the screen shows what is true now.
  const CHECK_BECOMES_INVESTIGATE_MS = 60000;

  /// Turn -> the question still being answered for it, or null.
  ///
  /// `t.question` is `{ kind, registeredAtMs }` and is set by `main.js` from the assignment
  /// register whenever an OPEN `check` or `investigate` names this turn. It is cleared the
  /// moment the register stops saying so — which is how the timer stops: the answer arriving
  /// settles the assignment, the next read drops it, and the row goes back to its own
  /// completed duration.
  function questionInFlight(t) {
    const q = t && t.question;
    if (!q || (q.kind !== "check" && q.kind !== "investigate")) return null;
    if (typeof q.registeredAtMs !== "number") return null;
    return q;
  }

  /// **THE WORD BESIDE THE TIMER WHILE A QUESTION IS BEING ANSWERED.**
  ///
  /// §58: *"in this case instead of 'working' it would say 'investigating' next to the
  /// timer"*, and *"'I'll check' is for when the answer from back-end is expected quickly"*.
  /// "Working" stays for tasks and never appears beside a question.
  ///
  /// **The elapsed time is measured from when the question was WRITTEN DOWN, not from when
  /// this row was drawn.** The register's `registered_at_ms` is fsynced before the front desk
  /// says "I'll check.", so the timer counts from the instant he was answered, and a window
  /// he backgrounded and came back to shows the true elapsed time rather than a fresh clock
  /// — the same derive-from-timestamps rule §6.2 already holds this file to.
  ///
  /// **The flip is mechanical and has a measurable worst case.** `updateTimers` runs on a
  /// 1,000 ms interval (`main.js`'s `startOrStopTimer`), so a check registered at t=0 first
  /// renders "Investigating for …" on the first tick at or after t=60,000 ms — worst case
  /// t=60,999 ms, i.e. up to 999 ms late and never early. `formatDuration` floors to whole
  /// seconds, so the number beside it reads `1m 0s` at the moment the word changes and the
  /// two cannot disagree.
  function questionRow(q, nowMs) {
    const elapsed = nowMs - q.registeredAtMs;
    const digging = q.kind === "investigate" || elapsed >= CHECK_BECOMES_INVESTIGATE_MS;
    const word = digging ? "Investigating" : "Checking";
    const d = formatDuration(elapsed);
    return {
      label: d ? `${word} for ${d}` : word,
      duration: d,
      tone: "active",
      // It claims exactly what is known: the question is with the other Rich, and nothing
      // about it is finished. No identifier, and it means the same read or spoken.
      note: digging
        ? "Rich is looking into this for you. You'll get the answer here."
        : "Rich is checking this for you. You'll get the answer here.",
      live: true,
    };
  }

  function durationRow(t, nowMs) {
    // **§58 OUTRANKS THE TURN'S OWN DURATION, and it has to.** The front desk's turn ENDED
    // when it said "I'll check." — §55's whole point — so this row would otherwise read
    // "Worked for 2s" beside a question that is still being answered. The turn really did
    // take two seconds and that is not what he needs to be looking at.
    const q = questionInFlight(t);
    if (q) return questionRow(q, nowMs);
    // §56: a job this turn gave is waiting for the screen, or has just carried on after it.
    if (t.screen === "waiting") {
      return { label: "Waiting for the screen to unlock", duration: null, tone: "active",
        note: "I'll carry on the moment it's back.", live: false };
    }
    if (t.screen === "back") {
      return { label: "The screen is back, so this is carrying on", duration: null, tone: "active", note: "", live: false };
    }
    switch (t.status) {
      case "queued":
      case "working":
      case "recovering": {
        // §14: a turn that was in flight when the process died is NOT running now, and
        // nothing recorded when it stopped. Refuse both the timer and the claim.
        if (!t.live) {
          return {
            label: "Status unavailable",
            duration: null,
            tone: "unknown",
            note: "This turn was still running when RichOS last closed and nothing recorded how it ended.",
            live: false,
          };
        }
        if (t.status === "queued") {
          return { label: "Waiting to start", duration: null, tone: "active", note: "Your message is recorded.", live: true };
        }
        // HIS ANSWER IS COMPLETE (product hunt 2026-10-15, finding 21): the turn finishes out of
        // sight, so the timer stops being shown. The measured "Worked for" arrives at its end.
        if (t.answered) {
          return { label: "", duration: null, tone: "done", note: DURATION_MEANING, live: false };
        }
        if (typeof t.startedAt !== "number") {
          // Accepted but not yet handed to a lease: §6.1's under-one-second row.
          return { label: "Working", duration: null, tone: "active", note: DURATION_MEANING, live: true };
        }
        const d = formatDuration(nowMs - t.startedAt);
        return {
          label: d ? `Working for ${d}` : "Working",
          duration: d,
          tone: "active",
          note: DURATION_MEANING,
          live: true,
        };
      }
      // A COMPLETED TURN WITH NO NUMBER TO SHOW HAS NO LABEL (product hunt 2026-10-15,
      // finding 13). It used to read "Worked", which says nothing: on nightly 57 it stood
      // above every message Rich sent on his own ("Rich reached out"), whose turns run for
      // well under a second. `renderDurationRow` draws no row for an empty label.
      case "completed": {
        if (typeof t.activeMs !== "number") {
          return {
            label: "",
            duration: null,
            tone: "done",
            note: "How long this took was not recorded.",
            live: false,
          };
        }
        const d = formatDuration(t.activeMs);
        // MEASURED but under §6.2's one-second display floor. Not the same statement as
        // "unrecorded", so it carries no apology.
        return {
          label: d ? `Worked for ${d}` : "",
          duration: d,
          tone: "done",
          note: DURATION_MEANING,
          live: false,
        };
      }
      // §11 `stopping`: "Composer disabled briefly | Timer Running | Stop mark". This is
      // the ONE status the renderer sets from a command RETURN rather than from an event —
      // and it is not a guess: `stop_turn` only answers once the request is fsync'd, so
      // "you asked me to stop" is a durable fact by the time this renders. The
      // AUTHORITATIVE terminal still arrives as `rich://turn-status: stopped` and replaces
      // it.
      case "stopping": {
        if (typeof t.startedAt !== "number") {
          return { label: "Stopping", duration: null, tone: "active", note: STOP_MEANING, live: true };
        }
        const d = formatDuration(nowMs - t.startedAt);
        return {
          label: d ? `Stopping · ${d}` : "Stopping",
          duration: d,
          tone: "active",
          note: STOP_MEANING,
          live: true,
        };
      }
      // §6.1's ATTRIBUTION row, and the only place in this file that names the CEO as the
      // cause of anything. It is rendered from `TurnState::Stopped`, which the ledger
      // writes only from a stop request that was fsync'd before anything was interrupted
      // (`steering.rs`). Slice 5 could not draw this label at all — `Interrupted` covered
      // a crash, a rotation and a cancel alike, so the sentence would have blamed the CEO
      // for a compute-lease failure.
      case "stopped": {
        if (typeof t.activeMs !== "number") {
          // A queued stop or a restart without an observed end time has no measured
          // span. Missing duration does not prove the work never started.
          return {
            label: "You stopped it",
            duration: null,
            tone: "ceo-stopped",
            note: "How long this took was not recorded.",
            live: false,
          };
        }
        const d = formatDuration(t.activeMs);
        return {
          label: d ? `You stopped after ${d}` : "You stopped it",
          duration: d,
          tone: "ceo-stopped",
          note: DURATION_MEANING,
          live: false,
        };
      }
      case "interrupted":
      case "failed": {
        if (typeof t.activeMs !== "number") {
          return {
            label: "Stopped before it finished",
            duration: null,
            tone: "stopped",
            note: "How long it ran was not recorded: the turn ended without writing an end time.",
            live: false,
          };
        }
        const d = formatDuration(t.activeMs);
        return {
          label: d ? `Stopped after ${d}` : "Stopped",
          duration: d,
          tone: "stopped",
          note: DURATION_MEANING,
          live: false,
        };
      }
      default:
        return { label: "Status unavailable", duration: null, tone: "unknown", note: "", live: false };
    }
  }

  // -------------------------------------------------------------------------------------
  // §5.3 SEMANTIC ACTIVITY ROLLUP
  // -------------------------------------------------------------------------------------
  //
  // §25: *"Grouped start and completion summaries avoid repetitive rows."* §5.3: *"one row
  // per meaningful action cluster."*
  //
  // The rollup counts TOOL CALLS, which is the only thing it can count: a CEO view carries
  // `summary` and no `detail`, so there are no file paths here to add up (the bytes were
  // removed by `Timeline::view`, not hidden). Each plural below is therefore true by
  // construction — n rows that each said "Read a file" is n files.
  //
  // A row whose summary already carries its own count ("Read 8 files") is NEVER merged with
  // another, because 8 + 8 is a sum this file cannot verify. It stays its own row.
  const PLURALS = {
    "Ran a command": (n) => `Ran ${n} commands`,
    "Read a file": (n) => `Read ${n} files`,
    "Edited a file": (n) => `Edited ${n} files`,
    "Searched": (n) => `Searched ${n} times`,
    "Used the web": (n) => `Used the web ${n} times`,
    "Viewed an image": (n) => `Viewed ${n} images`,
    "Used an integration": (n) => `Used ${n} integrations`,
    "Updated a thread": (n) => `Updated ${n} threads`,
    "Set up the environment": (n) => `Set up the environment (${n} steps)`,
    "Worked": (n) => `Worked (${n} steps)`,
  };

  /// The group's state from its members'. THE PRECEDENCE IS "REPORT THE WEAKEST CLAIM",
  /// not "report the most interesting one":
  ///
  ///   failed  -> he needs to know something broke
  ///   running -> ONLY if a member is explicitly `running`
  ///   unknown -> nobody recorded how at least one of these ended
  ///   queued  -> at least one opened and has not reported back
  ///   completed -> and only when EVERY member says so
  ///
  /// CORRECTED after rendering the real backend payload: a group of one `unknown` and one
  /// `queued` reported `running`, because the first clause tested them together. Nothing in
  /// that group was running. `running` is a statement that work is happening right now, and
  /// the emission probe found `in_progress` did not appear ONCE in 58 measured tool events
  /// — so a rule that reaches `running` from anything other than a literal `running` is a
  /// rule that will be wrong every single time it fires.
  ///
  /// `unknown` is COMMON and is not an error (34 of 58 events carried no `status` at all)
  /// and is NEVER folded into `completed`: "they all finished" is a completion claim nobody
  /// made, and §22 lists completion state under "must not be faked".
  function groupState(states) {
    if (states.includes("failed")) return "failed";
    if (states.includes("running")) return "running";
    if (states.includes("unknown")) return "unknown";
    if (states.includes("queued")) return "queued";
    if (states.length && states.every((s) => s === "completed")) return "completed";
    return "unknown";
  }

  /// Collapse a CONSECUTIVE run of WORK rows — activity and worker alike — into render
  /// groups, in order, never merging across the two kinds. A worker group and an activity
  /// group are different shapes with different truth conditions; running them through one
  /// rollup would let a worker state leak into an activity group state (or the reverse),
  /// which is the whole class of bug this file keeps refusing.
  function rollupWork(items) {
    const out = [];
    let run = [];
    let runIsWorker = null;
    const flushRun = () => {
      if (!run.length) return;
      if (runIsWorker) out.push({ type: "worker", group: rollupWorkers(run) });
      else for (const g of rollupActivity(run)) out.push({ type: "activity", group: g });
      run = [];
    };
    for (const item of items) {
      const w = isWorker(item);
      if (runIsWorker !== null && w !== runIsWorker) flushRun();
      runIsWorker = w;
      run.push(item);
    }
    flushRun();
    return out;
  }

  /// Collapse a run of CONSECUTIVE activity items with the identical summary into one row.
  /// Consecutive only — an interleaved prose run or a different activity type breaks the
  /// group, so the rollup can never reorder the chronology it is summarizing.
  ///
  /// A ROW CARRYING `detail` IS NEVER ROLLED UP. That is techy mode (§3.4: *"one collapsed
  /// line per tool call"*), and the rollup is the CEO view's answer to a different problem.
  /// `Read a file` three times says the same thing three times, so "Read 3 files" loses
  /// nothing; `cat engine/VERSION`, `cat partners.csv` and `cat notes.md` are three
  /// different facts, and folding them into "Ran 3 commands" would delete exactly what the
  /// technical view exists to show. Same data, same renderer, different question.
  function rollupActivity(items) {
    const out = [];
    for (const item of items) {
      const last = out[out.length - 1];
      if (last && !last.technical && !item.detail && last.summary === item.summary && PLURALS[item.summary]) {
        last.members.push(item);
        continue;
      }
      out.push({ summary: item.summary, activityType: item.activityType, members: [item], technical: !!item.detail });
    }
    return out.map((g) => ({
      key: g.members[0].id,
      count: g.members.length,
      activityType: g.activityType,
      // In technical mode the LABEL is what actually ran. The CEO-safe semantic summary
      // ("Ran a command") stays available on the member for the collapsed-turn line.
      label: g.technical
        ? technicalLabel(g.members[0])
        : g.members.length === 1
          ? g.summary
          : PLURALS[g.summary](g.members.length),
      state: groupState(g.members.map((m) => m.state)),
      members: g.members,
      technical: g.technical,
    }));
  }

  const ACTIVITY_STATE_LABEL = {
    queued: "queued",
    running: "running",
    completed: "done",
    failed: "failed",
    unknown: "outcome not recorded",
    stopped: "stopped",
  };

  // -------------------------------------------------------------------------------------
  // §7.1 DELEGATED AI WORKERS — and the one word this slice had to choose
  // -------------------------------------------------------------------------------------
  //
  // Three CEO-facing states exist, because three are all the engine can witness
  // (`richos_core::worker_events` — `created`, `started`, `updated`, `run_ended`). §7.1's
  // table lists seven. The four that are absent are absent for a sourced reason, not for
  // lack of effort:
  //
  //   Waiting      — the only candidate signal is `TeammateIdle`, whose payload cannot
  //                  separate "paused for input" from "finished for good".
  //   Done         — no completion signal exists at WORKER grain. `TaskCompleted` is
  //                  authoritative but task-grain, and `SubagentStop` is not a completion.
  //   Stopped      — a `shutdown_request` is an instruction issued BEFORE anything happens,
  //                  not an observation.
  //   Failed       — no payload in the worker path carries an outcome at all.
  //
  // ===== `run_ended` IS NOT `completed`, AND THIS IS THE WORDING THAT SAYS SO =============
  //
  // Every run that ends arrives as `run_ended` with the reason genuinely unobservable.
  // `richos_core::timeline::RUN_ENDED_WORKER_STATE` is `WorkerState::Unknown`, serialized
  // `"unknown"`, and it is a NAMED CONSTANT precisely so nobody defaults it into a
  // confident outcome. `completed`, `interrupted` and `failed` are all folded inside it, so
  // drawing any one of them would be wrong roughly two thirds of the time — and §7.4
  // renders failure and recovery completely differently from success, so a wrong collapse
  // does not mislabel a row, it draws the wrong thing.
  //
  // The word chosen is **`Ended`**, always paired with the visible qualifier
  // **`outcome not recorded`**. Three constraints, and why each candidate failed:
  //
  //   * must not imply success  -> rules out `Done`, `Finished`, `Completed`, `Wrapped up`.
  //     §7.1's own verb `{name} finished` is reserved for the `completed` state and taking
  //     it here would claim the one thing nobody witnessed.
  //   * must not imply failure  -> rules out `Stopped` (§7.1's word for `interrupted`,
  //     which asserts something cut it short) and anything in the danger palette.
  //   * must not sound broken   -> rules out `Unavailable` / `Unknown` / `?`. Those are
  //     §7.1's `not_found` treatment and they say "I lost track of this worker", which is
  //     false and alarming: the run's end was positively WITNESSED. What is missing is the
  //     reason, not the worker.
  //
  // `Ended` states exactly what was observed — the run is over — and claims nothing about
  // the work. `outcome not recorded` is the same vocabulary slice 5 already ships for an
  // activity row whose status never arrived (`ACTIVITY_STATE_LABEL.unknown`), so the CEO
  // learns one phrase, not two. It is rendered as TEXT, never color alone (§18), and it
  // is in the chip's accessible name.
  //
  // The group verb is `{names} are no longer running` for the same reason: §7.1's list
  // offers `finished`, `was interrupted` and `failed`, and all three are verdicts.
  // =======================================================================================

  const WORKER_STATES = {
    // §7.1 verbatim: "pending_init | Starting | Hollow indicator". The harness accepted the
    // spawn and returned an id; execution is NOT confirmed, and "Starting" is exactly that.
    pending_init: {
      label: "Starting",
      glyph: "○",
      tone: "starting",
      qualifier: null,
      note: "The spawn was accepted. Nothing has reported back yet.",
      pulse: false,
    },
    // §7.1 verbatim: "running | Working | Subtle pulse".
    running: {
      label: "Working",
      glyph: "◐",
      tone: "active",
      qualifier: null,
      note: "This run is open: no end has been recorded for it.",
      pulse: true,
    },
    // THE ONE THIS SLICE CHOSE. See the block above.
    unknown: {
      label: "Ended",
      glyph: "◇",
      tone: "ended",
      qualifier: "outcome not recorded",
      note:
        "This run has ended. Nothing recorded whether the work finished, stopped or failed, so I'm not going to call it either way.",
      pulse: false,
    },
  };

  /// Any state outside the three above cannot be produced by this runtime today
  /// (`WorkerState::from_observed` yields only `pending_init`, `running` and `unknown`).
  /// If one ever arrives it is a NEW signal whose treatment is a product decision that has
  /// not been made — so it reads as the one thing that is certainly true and claims
  /// nothing, exactly as the duration row's default does for an unrecognized turn state.
  function workerStateSpec(state) {
    return (
      WORKER_STATES[state] || {
        label: "Status unavailable",
        glyph: "·",
        tone: "unknown",
        qualifier: null,
        note: "This worker reported a state RichOS does not know how to read.",
        pulse: false,
      }
    );
  }

  /// **A teammate is shown by its name** (proto-teammate shelf plan §7): the engine starts a
  /// named teammate as `<teammate>-<model>-<id>` (`mark-sonnet-1a2b3c4d5e6f`), the shape
  /// `teammate-name.sh` enforces, and a person reads "Mark". Anything not of that shape is
  /// shown as it came; the Output panel's "by …" uses this same function.
  function teammateName(raw) {
    const m = /^([a-z][a-z0-9]{1,15})-[a-z]+-[a-z0-9]{1,12}$/.exec(raw || "");
    return m ? m[1].charAt(0).toUpperCase() + m[1].slice(1) : raw;
  }

  /// §7.1 wants a display NAME. `workerName` is carried only by a `created` row — a run
  /// first witnessed at `started` genuinely has none, and nothing invents one. The
  /// fallbacks descend through what was actually observed and stop before the `agentId`:
  /// an opaque harness id is machinery, and §5.3 keeps the CEO default semantic.
  function workerDisplayName(w) {
    if (w.workerName) return teammateName(w.workerName);
    if (w.agentType) return w.agentType;
    return "A teammate";
  }

  function workerHasName(w) {
    return !!(w.workerName || w.agentType);
  }

  /// "Sage", "Sage and Frank", "Sage, Frank and Clark" — §7.1's own example, verbatim.
  function joinNames(names) {
    if (names.length <= 1) return names[0] || "";
    if (names.length === 2) return names[0] + " and " + names[1];
    return names.slice(0, -1).join(", ") + " and " + names[names.length - 1];
  }

  /// §7.1: *"Grouped events should say `Sage, Frank and Clark started working`, not render
  /// three identical rows."*
  ///
  /// The verb comes from the states the group ACTUALLY holds, and there is deliberately no
  /// verb for a mixed group: "started working" alongside a worker that has already ended
  /// would be a claim about one member made from another's evidence. A mixed group states
  /// the one fact every member shares — Rich delegated to them — and the chips carry each
  /// individual state, which is where §25's "their states update independently" lives.
  function workerGroupSummary(workers) {
    const names = workers.map(workerDisplayName);
    const who = joinNames(names);
    const states = workers.map((w) => w.state);
    const all = (s) => states.length > 0 && states.every((x) => x === s);
    if (all("pending_init")) return who + (workers.length === 1 ? " is starting" : " are starting");
    // §7.1's verb list, verbatim.
    if (all("running")) return who + " started working";
    if (all("unknown")) return who + (workers.length === 1 ? " is no longer running" : " are no longer running");
    return "Delegated to " + who;
  }

  /// Collapse a run of CONSECUTIVE worker rows into one group, one chip per DISTINCT
  /// `agentId`. Consecutive only, for the same reason the activity rollup is: a group must
  /// never reorder the chronology it summarizes. Distinct by agent id because two `Task`
  /// calls naming the same worker are two records of one worker, not two workers — the
  /// join key is the identity (§7.2), never the name and never the row.
  function rollupWorkers(items) {
    const seen = new Map();
    const order = [];
    for (const item of items) {
      const w = item.worker;
      if (!w || !w.agentId) continue;
      if (!seen.has(w.agentId)) order.push(w.agentId);
      // Last write wins: a later record carries the later observation.
      seen.set(w.agentId, w);
    }
    const workers = order.map((id) => seen.get(id));
    return {
      key: items[0] ? items[0].id : "workers",
      workers,
      label: workerGroupSummary(workers),
    };
  }

  // -------------------------------------------------------------------------------------
  // THE MODEL
  // -------------------------------------------------------------------------------------

  /// The synthetic turn id an optimistic CEO bubble carries between "the CEO pressed Enter"
  /// and "the spine told us the turn id". It is re-keyed onto the real turn the moment
  /// `rich://turn-status` names one, so the bubble is never drawn twice.
  ///
  /// It is the PREFIX of each bubble's own pending turn (`addPendingUserMessage`): a turn draws
  /// one CEO message, so two bubbles sharing one pending turn drew only the first. Two Sends
  /// pressed before the launch had opened his conversation then showed one message for as long
  /// as the launch took (handover review rv-20261020T191206Z-fa2ee4a4-778c).
  const PENDING_TURN = "\u0000pending";

  const SLOT_RANK = { opening: 0, stream: 1, terminal: 2 };

  function createModel() {
    return {
      entityId: null,
      threadId: null,
      /// The activation revision we are fenced at. §13's fence is a STALENESS fence, never
      /// an equality key — see `accepts()`.
      bindingRevision: -1,
      items: new Map(), // id -> item  (idempotent by id: §13 "repeated event IDs are idempotent")
      turns: new Map(), // turnId -> { status, startedAt, activeMs, live, mergedFrom: [] }
      turnOrder: [], // turnIds, oldest first
      expanded: new Set(), // turnIds whose work transcript the CEO has opened
      /// turnIds the CEO has explicitly CLOSED. Not the complement of `expanded`: a turn
      /// in neither set has never been touched, and §6.4 gives an untouched turn two
      /// different defaults depending on whether it is running. See `isTurnExpanded`.
      collapsed: new Set(),
      settled: new Set(), // turnIds whose post-completion settle has already run
      announcedWorking: new Set(),
      /// turnId -> `{ kind, registeredAtMs }` for a QUESTION of his that is still being
      /// answered (CEO ruling §58). Written only by `setQuestionsInFlight`, from the
      /// assignment register; never inferred from anything on the timeline.
      ///
      /// **It lives on the MODEL and is carried across `applySnapshot`, like `expanded` and
      /// `settled`** — `model.turns` is rebuilt from scratch on every snapshot, so a flag
      /// stamped only on a turn record would be wiped the next time he said anything, which
      /// is exactly the way a background result was being destroyed before 2026-09-17.
      questions: new Map(),
      /// turnId -> "waiting" | "back" for a job that turn gave which waits for the screen, or
      /// has just carried on after it (CEO ruling §56). Written only by `setScreenWaits`, from
      /// the assignment register, and carried across `applySnapshot` for the reason above.
      screenWaits: new Map(),
      pendingUser: [], // optimistic CEO bubbles awaiting a turn id
      /// TRUE when the snapshot in this model came from `get_machinery` — i.e. the CEO has
      /// techy mode on for this thread. Read in exactly one place, `isTurnExpanded`, and
      /// set in exactly one place, `applySnapshot`, from the payload's own `mode` field. It
      /// is never inferred from the presence of a `detail` somewhere.
      technical: false,
      /// Machinery ids whose RAW pane the CEO has opened (techy mode §3.4: "Expand for
      /// input/output"). Held on the model, not in a render-local variable, so a stream of
      /// new rows during a live turn does not close a pane he is reading.
      expandedMachinery: new Set(),
    };
  }

  function bind(model, entityId, threadId, bindingRevision) {
    model.entityId = entityId;
    model.threadId = threadId;
    model.bindingRevision = typeof bindingRevision === "number" ? bindingRevision : -1;
  }

  /// THE FENCE (§13: *"The renderer rejects events that do not match the immutable
  /// binding."*), applied exactly as STREAMING.md specifies and NOT as it reads at first
  /// glance:
  ///
  ///   * EQUALITY is `entityId` + `threadId`. Both are immutable for the life of a thread.
  ///   * `bindingRevision` is a STALENESS fence, NEVER an equality key. It carries the
  ///     revision of the ACTIVATION that produced the event, so it advances on every thread
  ///     activation and is legitimately HIGHER than the revision a re-projection of the
  ///     same thread reports. Comparing it for equality would reject every live event after
  ///     any thread switch — i.e. the app would go silent the first time the CEO changed
  ///     threads and came back. Reject anything OLDER; accept at or above.
  function accepts(model, payload) {
    if (!payload) return false;
    if (model.threadId == null) return false;
    if (payload.threadId !== model.threadId) return false;
    if (model.entityId != null && payload.entityId !== model.entityId) return false;
    if (typeof payload.bindingRevision === "number" && payload.bindingRevision < model.bindingRevision) {
      return false;
    }
    return true;
  }

  /// WHICH ITEMS THIS MODEL MAY HOLD. Belt AND braces: the spine already refuses to put a
  /// technical or internal item on the live family (`LiveEvent::may_reach_webview`) and
  /// `Timeline::view(Ceo)` already removes them from a CEO snapshot, so on the calm path
  /// this should never fire. It exists so that if a future emitter widens the family, the
  /// calm view does not silently start rendering machinery — slice 3 fixed a live defect
  /// where untyped vendor kinds surfaced as six CEO rows reading "Worked" against one real
  /// command, and that class of regression must fail closed here too.
  ///
  /// **`technical` is admitted ONLY when the model is holding a technical snapshot**, which
  /// `applySnapshot` sets from the payload's own `mode` and nothing else sets at all. So:
  ///
  ///   * a CEO snapshot cannot bring a technical row in, whatever it contains;
  ///   * a LIVE event cannot either — the live family is `"ceo"` by construction, and a
  ///     widened emitter would still have to get past this on a calm model;
  ///   * `internal` is refused in EVERY mode, exactly as `Visibility::renders_in` refuses
  ///     it in every `ViewMode` (timeline.rs). Re-prime and rotation machinery has no
  ///     render path here either, and techy mode does not open one.
  function visible(model, item) {
    if (!item || item.visibility === undefined) return true;
    if (item.visibility === "ceo") return true;
    return item.visibility === "technical" && model.technical === true;
  }

  /// THE ORDER GUARD IS ON THE CREATE BRANCH, AND THAT IS NOT A SHORTCUT. Every site that
  /// removes a turn id from `turnOrder` removes its record in the same breath (`dropTurn`,
  /// and `onTurnStatus`'s merge, which re-inserts it), so `model.turns.has(id)` implies
  /// `turnOrder` already carries it — and re-scanning the array for an id we have just been
  /// handed a record for cost O(turns) per ITEM. On the 10,000-item thread the design record
  /// promises that was 271 ms of `applySnapshot` before a single node existed
  /// (`docs/verification/timeline-scale-2026-08-30/baseline.txt`). `scale.js` pins the
  /// implication itself, not just the timing.
  function turnRecord(model, turnId) {
    let t = model.turns.get(turnId);
    if (!t) {
      t = { status: "queued", startedAt: null, activeMs: null, live: false, mergedFrom: [] };
      model.turns.set(turnId, t);
      if (!model.turnOrder.includes(turnId)) model.turnOrder.push(turnId);
    }
    return t;
  }

  /// Upsert by id. A repeated id is idempotent (§13), and a later payload for the same id
  /// wins field by field.
  ///
  /// EXCEPT WHEN THE KIND CHANGES, in which case the new item REPLACES the old one rather
  /// than merging over it. That is a real transition, not a defensive nicety: one machinery
  /// record is one row, and a delegation whose lifecycle row lands after its tool result
  /// arrives first as `activity` and is upgraded to `worker_activity` under the SAME id
  /// (live.rs, "the lifecycle row can arrive after the tool result"). Merging would leave
  /// the activity row's `summary`, `state` and `activityType` clinging to a worker row —
  /// fields whose vocabularies are different and whose values would be stale.
  function putItem(model, item) {
    if (!visible(model, item)) return false;
    const prev = model.items.get(item.id);
    const merged = prev && prev.kind === item.kind ? Object.assign({}, prev, item) : item;
    model.items.set(item.id, merged);
    placeTurn(model, item);
    return !prev || prev.kind !== item.kind; // structural change?
  }

  /// **A QUESTION GIVES ITS TURN A PLACE, NEVER A RECORD** (nightly-38 walk, D8).
  ///
  /// A turn record is the ledger's word on a turn: `work_duration` (every ledger turn has
  /// one, `Timeline::project`) or a live `turn-status`. A question card is the one item on
  /// the wire that is NOT projected from a ledger turn — `question_host.rs` appends it from
  /// the question store, and a background job's question carries its OBLIGATION as `turnId`
  /// (`native.rs` `prepare_question_scope`), which no ledger turn has. Recording it here
  /// gave that id `turnRecord`'s defaults, `queued` and not live, which is precisely a turn
  /// a quit left running: "Status unavailable" and the quit card were drawn under a job's
  /// open question while RichOS had never closed.
  ///
  /// So a question only takes its slot in the order (where it was asked, between the turns
  /// around it) and is drawn by `renderTurn`'s no-record lane, like a local notice. A
  /// question inside a real turn loses nothing: that turn's `work_duration` row records it.
  function placeTurn(model, item) {
    if (!item.turnId) return;
    // A file Rich or Stu attached (`rich_files_host.rs`) is appended the same way a question
    // is, and a background job's file carries its job as `turnId` just as its question does.
    // So is the "Your instructions · Saved" card (`your_instructions.rs`, round 36 screen 1).
    if (item.kind !== "question" && item.kind !== "rich_file" && item.kind !== "instructions_saved") {
      turnRecord(model, item.turnId);
      return;
    }
    if (!model.turns.has(item.turnId) && !model.turnOrder.includes(item.turnId)) {
      model.turnOrder.push(item.turnId);
    }
  }

  // ---- the reload path -----------------------------------------------------------------

  /// Replace the model from a `get_timeline` snapshot (§14 step 2, §13 *"reconnect uses a
  /// full snapshot"*).
  ///
  /// This deliberately does NOT merge: the snapshot is the durable truth, and anything the
  /// live path invented that the ledger does not hold should disappear rather than linger.
  /// Item ids are derived from the durable records (`{turnId}:user`, `{turnId}:text:{n}`,
  /// `{turnId}:duration`, the machinery id), so a snapshot taken mid-session re-states what
  /// is already on screen instead of duplicating it.
  ///
  /// LIVE STATE SURVIVES IT. A turn still streaming in this session keeps `live: true` and
  /// its `startedAt`, because the snapshot's `working` row carries no information about
  /// whether the lease is alive right now — only this session knows that.
  function applySnapshot(model, snapshot) {
    const previousItems = model.items;
    const sameView = snapshot &&
      (snapshot.entityId == null || snapshot.entityId === model.entityId) &&
      (snapshot.threadId == null || snapshot.threadId === model.threadId) &&
      (snapshot.mode === "technical") === model.technical;
    const liveTurns = new Map();
    for (const [id, t] of model.turns) if (t.live) liveTurns.set(id, t);
    // **LOCAL NOTICES SURVIVE THE SNAPSHOT, and until 2026-09-17 they did not — which
    // DESTROYED a background result the moment he said anything** (Frank's check of the Two
    // Riches spec, defect (a)). The path: a result is taken from the durable register by
    // `take_work_notices`, which marks it delivered ONCE; it is rendered through
    // `addLocalNotice` into this model; and his next sentence ends in
    // `rich://turn-completed`, which calls `loadTimeline()` -> this function, which used to
    // clear `model.items` outright. The notice was gone from the screen and already marked
    // delivered, so nothing would ever show it again.
    //
    // **Carrying them over does not make them evidence.** They keep their synthetic turn id,
    // they are still in no ledger, they still grow no duration row, and they are still gone
    // when the process is — what changes is that a reload of the thread he is looking at no
    // longer deletes something he was told.
    const carried = [];
    for (const [id, item] of model.items) if (item.kind === "local_notice") carried.push([id, item]);
    const expanded = model.expanded;
    const collapsed = model.collapsed;
    const settled = model.settled;
    const announced = model.announcedWorking;
    const questions = model.questions;
    const screenWaits = model.screenWaits;

    model.items = new Map();
    model.turns = new Map();
    model.turnOrder = [];
    model.expanded = expanded;
    model.collapsed = collapsed;
    model.settled = settled;
    model.announcedWorking = announced;
    model.questions = questions;
    model.screenWaits = screenWaits;
    model.pendingUser = [];

    if (!snapshot || !Array.isArray(snapshot.items)) return;
    model.entityId = snapshot.entityId != null ? snapshot.entityId : model.entityId;
    model.threadId = snapshot.threadId != null ? snapshot.threadId : model.threadId;
    // `TimelineView.mode` — `"ceo"` or `"technical"` — straight off the payload the backend
    // built. See `isTurnExpanded` for the one thing it changes.
    model.technical = snapshot.mode === "technical";

    for (const raw of snapshot.items) {
      if (!visible(model, raw)) continue;
      if (raw.kind === "work_duration") {
        const t = turnRecord(model, raw.turnId);
        t.status = raw.state; // queued | working | completed | interrupted
        t.startedAt = typeof raw.startedAt === "number" ? raw.startedAt : null;
        t.activeMs = typeof raw.activeMs === "number" ? raw.activeMs : null;
        const alive = liveTurns.get(raw.turnId);
        if (alive && (raw.state === "queued" || raw.state === "working")) {
          t.live = true;
          if (typeof alive.startedAt === "number") t.startedAt = alive.startedAt;
        }
        continue;
      }
      // Rendering signs items by object identity. A fresh wire object with exactly
      // the same content must not replace an active control in the same view.
      // Rebuild membership from the snapshot regardless: removals and changed values
      // remain authoritative, with no partial merge of the previous item.
      const previous = sameView && previousItems.get(raw.id);
      putItem(model, previous && JSON.stringify(previous) === JSON.stringify(raw) ? previous : raw);
    }
    // A turn that contributed only a duration row still needs its place in the order.
    for (const raw of snapshot.items) placeTurn(model, raw);
    // §58: the questions still in flight go back onto the rebuilt records. Last, so every
    // turn the snapshot mentions already exists.
    stampQuestions(model);
    restoreLocalNotices(model, carried);
  }

  /// Copy `model.questions` onto the turn records it names. A question whose turn is not in
  /// this thread's snapshot is left in the map untouched rather than dropped: `get_assignments`
  /// is per thread and this is the same thread, so a missing turn means the snapshot has not
  /// reached that far back, not that the question is over.
  function stampQuestions(model) {
    for (const [, t] of model.turns) if (t.question) t.question = null;
    for (const [turnId, q] of model.questions) {
      const t = model.turns.get(turnId);
      if (t) t.question = q;
    }
    for (const [, t] of model.turns) if (t.screen) t.screen = null;
    for (const [turnId, s] of model.screenWaits || []) {
      const t = model.turns.get(turnId);
      if (t) t.screen = s;
    }
  }

  /// **A JOB WAITING FOR THE SCREEN, ON THE TIMELINE** (CEO ruling §56). Each row that names a
  /// turn in this thread and is `waiting-for-screen` puts "Waiting for the screen to unlock" on
  /// that turn's line; once the screen is back the record says so (`screen::says::carrying_on`)
  /// until Stu reports on the job, and the line says it is carrying on. Then the line goes back
  /// to the turn's own duration. It asks nothing of him and has no timer.
  const SCREEN_BACK_DETAIL = "The screen is back, so this is carrying on.";
  function setScreenWaits(model, rows, threadId) {
    const next = new Map();
    const prefix = "ledger:" + threadId + ":";
    for (const row of rows || []) {
      if (typeof row.turnRef !== "string" || !row.turnRef.startsWith(prefix)) continue;
      const turnId = row.turnRef.slice(prefix.length);
      if (!turnId) continue;
      if (row.state === "waiting-for-screen") next.set(turnId, "waiting");
      else if (row.state === "registered" && row.detail === SCREEN_BACK_DETAIL && next.get(turnId) !== "waiting") {
        next.set(turnId, "back");
      }
    }
    model.screenWaits = next;
    stampQuestions(model);
  }

  /// **THE QUESTIONS STILL BEING ANSWERED, from the assignment register** (CEO ruling §58).
  ///
  /// `rows` is what `get_assignments` returned. Each row that is an OPEN `check` or
  /// `investigate`, and names a turn in this thread, puts a live timer beside that turn's
  /// reply; everything else is ignored. **The answer arriving is what stops the timer** —
  /// the assignment settles, the next read no longer lists it as open, and this clears it.
  /// Nothing here times anything out and nothing here decides a question is over.
  ///
  /// `threadId` is required and is compared, not trusted: the reference the register stores
  /// is `ledger:<thread>:<turn>`, so an assignment from another conversation cannot name a
  /// turn in this one even by accident.
  function setQuestionsInFlight(model, rows, threadId) {
    const next = new Map();
    for (const row of rows || []) {
      if (row.kind !== "check" && row.kind !== "investigate") continue;
      if (!["registered", "preparing", "running"].includes(row.state)) continue;
      if (typeof row.turnRef !== "string" || typeof row.registeredAtMs !== "number") continue;
      const prefix = "ledger:" + threadId + ":";
      if (!row.turnRef.startsWith(prefix)) continue;
      const turnId = row.turnRef.slice(prefix.length);
      if (!turnId) continue;
      next.set(turnId, { kind: row.kind, registeredAtMs: row.registeredAtMs });
    }
    model.questions = next;
    stampQuestions(model);
  }

  /// Is anything on this timeline ticking — a live turn, or a question being answered?
  ///
  /// `main.js` runs one interval for the whole timeline and only while something moves. It
  /// used to ask `t.live` directly; §58 added a second reason a row ticks, and asking here
  /// keeps the two answers from drifting apart (`updateTimers` asks the same question).
  function hasLiveRow(model) {
    for (const t of model.turns.values()) if (t.live || questionInFlight(t)) return true;
    return false;
  }

  /// WHERE A SYNTHETIC TURN GOES IN HIS SCROLLBACK: before the first turn that started after
  /// it, by the earliest `createdAt` the snapshot gives that turn, and at the end only when
  /// nothing on screen is newer — which is the ordinary case for something that has just
  /// happened.
  ///
  /// Appending unconditionally would be simpler and would read as a lie: a failure from six
  /// minutes ago sitting under the newest answer says "this just happened". That is audit-7
  /// row 9 — after a relaunch the two failure cards came back at the END of his thread, after
  /// "Paris.", instead of where they happened, so "his scrollback no longer matches the order
  /// of events".
  ///
  /// IT IS A SHARED FUNCTION NOW, AND THAT IS THE WHOLE OF THE ROW-9 FIX. This arithmetic was
  /// written for `restoreLocalNotices` — a notice CARRIED ACROSS A SNAPSHOT within one session,
  /// where it keeps the `createdAt` it was born with — and `addLocalNotice`, the path every
  /// notice is born on, pushed to the end regardless. Within a session that is right, because
  /// a notice is born now. Across a RELAUNCH it is not: `take_work_notices` hands back
  /// everything he has not been told, each carrying `raisedAtMs`, and those are minutes or days
  /// old. One placement rule, used by both, and neither can drift from the other.
  function placeByTime(model, turnId, at) {
    if (model.turnOrder.includes(turnId)) return;
    const startedAt = new Map();
    for (const item of model.items.values()) {
      if (!item.turnId || item.turnId === turnId) continue;
      const t = typeof item.createdAt === "number" ? item.createdAt : null;
      if (t == null) continue;
      const known = startedAt.get(item.turnId);
      if (known == null || t < known) startedAt.set(item.turnId, t);
    }
    const before = model.turnOrder.findIndex((id) => {
      const turnAt = startedAt.get(id);
      return turnAt != null && turnAt > at;
    });
    if (before < 0) model.turnOrder.push(turnId);
    else model.turnOrder.splice(before, 0, turnId);
  }

  /// Put the carried local notices back, in time order rather than at the end.
  function restoreLocalNotices(model, carried) {
    if (!carried.length) return;
    for (const [id, item] of carried) {
      model.items.set(id, item);
      placeByTime(model, item.turnId, typeof item.createdAt === "number" ? item.createdAt : 0);
    }
  }

  // ---- the live path -------------------------------------------------------------------

  /// The CEO pressed Enter. The bubble goes up immediately (§25: *"renders immediately"*)
  /// with a synthetic id, and is re-keyed onto the real turn as soon as one is named.
  function addPendingUserMessage(model, text, at) {
    const id = "pending:" + model.pendingUser.length + ":" + at;
    const item = {
      kind: "user_message",
      id,
      entityId: model.entityId,
      threadId: model.threadId,
      turnId: PENDING_TURN + ":" + id,
      createdAt: at,
      slot: "opening",
      sequence: null,
      visibility: "ceo",
      text,
      source: "text",
      pending: true,
    };
    model.items.set(id, item);
    model.pendingUser.push(id);
    return id;
  }

  /// §11 `stopping`, set from `stop_turn`'s successful RETURN.
  ///
  /// The one status this renderer sets without an event, and the reason it is honest: the
  /// command does not answer until the stop request is fsync'd, so by the time this runs,
  /// "you asked me to stop" is a durable fact rather than an optimistic guess. The
  /// authoritative terminal still arrives as `rich://turn-status: stopped` and replaces it.
  /// Applied only to a turn that is currently live, so it can never resurrect a finished
  /// row.
  function markStopping(model, turnId) {
    const t = model.turns.get(turnId);
    if (!t || !t.live) return false;
    t.status = "stopping";
    return true;
  }

  /// A locally-authored line that is NOT in the ledger — a voice-mode failure, a send that
  /// never reached the desk, a background result he has just been handed. It carries a
  /// synthetic turn id with no turn record, so it can never grow a duration row claiming work
  /// that never happened.
  ///
  /// **It used to say "and the next snapshot drops it (it is not evidence)", and that
  /// sentence was a defect rather than a design** (Frank's check of the Two Riches spec,
  /// defect (a)): a background result is handed over ONCE by `take_work_notices`, and his
  /// next sentence reloads the thread, so the drop destroyed something he had been told and
  /// nothing would ever show it again. `applySnapshot` carries them now; they are still in no
  /// ledger and still gone with the process.
  ///
  /// **IT IS ITS OWN KIND, AND THAT IS RAY'S CANDIDATE-.4 FINDING #8.** Until 2026-09-17 it
  /// was written into the model as `rich_message`, so `renderRichMessage` stamped it with
  /// Rich's byline and avatar and gave it a **Copy Rich's message** button. A microphone
  /// status line was therefore indistinguishable, reading back, from an answer Rich gave —
  /// and the two are not the same thing at all: one is in the ledger and is evidence of what
  /// he was told, the other is the app talking about itself and is gone on the next
  /// snapshot. The sentences are unchanged; what changed is that they no longer claim to be
  /// speech. See `renderLocalNotice`.
  function addLocalNotice(model, text, at) {
    const turnId = "\u0000local:" + at + ":" + Math.random().toString(36).slice(2, 8);
    const id = turnId + ":text:0";
    const item = {
      kind: "local_notice",
      id,
      entityId: model.entityId,
      threadId: model.threadId,
      turnId,
      createdAt: at,
      slot: "stream",
      sequence: null,
      visibility: "ceo",
      phase: "unknown",
      text,
      closed: true,
    };
    model.items.set(id, item);
    // BY WHEN IT HAPPENED, NOT AT THE END — audit-7 row 9. `at` is now for a notice that has
    // just been raised, so this pushes to the end exactly as it always did for the live path.
    // What it stops doing is putting a notice the BACKEND has been holding since before the
    // relaunch under the newest answer: `take_work_notices` hands back everything he has not
    // been told, each `PendingNotice` carrying its own `raisedAtMs`, and Ray watched two
    // failures from `08:13` and `08:14` come back after an answer given at `08:16`.
    placeByTime(model, turnId, at);
    return id;
  }

  /// Adopt the oldest un-adopted optimistic bubble onto a real turn, under the id the
  /// ledger will derive for it. Doing this — rather than leaving the placeholder and adding
  /// the projected item later — is what stops the CEO's one sentence appearing twice.
  function adoptPendingUserMessage(model, turnId) {
    // queued and working both arrive for one turn. The second event must not consume
    // the next request's words merely because this turn already has its own bubble.
    if (model.items.has(`${turnId}:user`)) return;
    const pendingId = model.pendingUser.shift();
    if (!pendingId) return;
    const item = model.items.get(pendingId);
    if (!item) return;
    model.items.delete(pendingId);
    const realId = `${turnId}:user`;
    if (!model.items.has(realId)) {
      model.items.set(realId, Object.assign({}, item, { id: realId, turnId, pending: false }));
    }
  }

  /// `rich://ceo-message` — **WHAT HE SAID, ON A SURFACE THAT DID NOT WATCH HIM SAY IT.**
  ///
  /// Every other event in this family is about Rich, and for good reason while the only
  /// surface was a window that had drawn the CEO's sentence itself the moment he pressed
  /// Send. His phone made that false. Ray's candidate .13, defect R3: he typed on his phone
  /// and the Mac showed nothing for **5.68 s** — measured twice, 5.77 s and 5.68 s — and then
  /// his words and Rich's finished answer appeared in the SAME FRAME, because the only thing
  /// that ever put them on screen was `turn-completed`'s reload. The other direction takes
  /// 0.096 s. At 2.99 s the sidebar's working dot had moved and the thread had not, which is
  /// the proof the events were arriving and none of them carried his sentence.
  ///
  /// # It is an UPSERT on the projection's own id, never an insert
  ///
  /// `messageId` is `{turnId}:user` — the id `Timeline::project` derives — so for a message
  /// he typed HERE this event arrives after `turn-status: queued` has already adopted the
  /// optimistic bubble onto exactly that id, and there is nothing to do. One sentence stays
  /// one row, live and after a reload, with no de-duplication rule anywhere.
  ///
  /// # When the text disagrees, THE LEDGER WINS, and that is a real repair
  ///
  /// The one way a row can exist here with different words is a mis-adoption:
  /// `adoptPendingUserMessage` takes the OLDEST un-adopted bubble, so a `queued` for a turn
  /// that started on the phone can consume a sentence he is part-way through sending from the
  /// desk. That is a false attribution — his desk sentence rendered as the phone's turn — and
  /// this event is the first thing that has ever been able to see it, because it carries what
  /// the ledger actually stored. It is corrected here.
  ///
  /// **The residue, stated rather than buried:** the bubble that was wrongly consumed is not
  /// put back by this; it returns with its own turn's reload. That window is narrow (both
  /// mouths inside one turn boundary) and it is older than this event.
  function onCeoMessage(model, p) {
    if (!accepts(model, p)) return { structural: false, rejected: true };
    if (!p.messageId) return { structural: false, rejected: true };
    const existing = model.items.get(p.messageId);
    if (existing && existing.kind === "user_message") {
      if (existing.text === p.text) return { structural: false, rejected: false };
      model.items.set(p.messageId, Object.assign({}, existing, { text: p.text, source: p.source || existing.source }));
      return { structural: true, rejected: false };
    }
    const isNew = putItem(model, {
      kind: "user_message",
      id: p.messageId,
      entityId: p.entityId,
      threadId: p.threadId,
      turnId: p.turnId,
      // THE TURN'S OWN INSTANT, not `at`. `createdAt` is what `placeByTime` and the renderer
      // sort on, and the projection uses the turn's `created_at`; taking the emit instant
      // instead would sort the live row differently from the reloaded one.
      createdAt: typeof p.createdAt === "number" ? p.createdAt : p.at,
      slot: "opening",
      sequence: null,
      visibility: "ceo",
      text: p.text,
      source: p.source || "text",
      pending: false,
    });
    // The turn's place in `turnOrder` is `putItem`'s to make, through `turnRecord`, exactly as
    // it is for every other item — there is no second ordering rule here.
    return { structural: isNew, rejected: false };
  }

  /// WITHDRAW an optimistic bubble that will never become a turn.
  ///
  /// The counterpart to `adoptPendingUserMessage`, and it exists because `send()` can be
  /// refused BEFORE any turn starts: no lease, so no `turn-status` will ever name a turn
  /// for it and no snapshot will ever replace it. Left in place, the bubble sits on screen
  /// looking exactly like a message that went — which is the app telling the CEO his words
  /// were delivered when they were not.
  ///
  /// Only ever applied to the id `addPendingUserMessage` returned, and only while it is
  /// still `pending`: an adopted bubble belongs to a real turn and must never be removed
  /// by this path.
  function dropPendingUserMessage(model, id) {
    const item = model.items.get(id);
    if (!item || !item.pending) return false;
    model.items.delete(id);
    const at = model.pendingUser.indexOf(id);
    if (at >= 0) model.pendingUser.splice(at, 1);
    return true;
  }

  /// The command rejected but its terminal event did not reach this window. This is a
  /// local failure of that invocation, not an invented ledger event or measured duration.
  /// A subsequent authoritative snapshot replaces it. Keep accepted words and partial work.
  function markSendRejected(model, turnId) {
    const t = model.turns.get(turnId);
    if (!t || !t.live) return false;
    t.status = "failed";
    t.live = false;
    t.activeMs = null;
    return true;
  }

  /// Drop every trace of a turn — items, record and order slot. Used only by the
  /// supersession merge below.
  function dropTurn(model, turnId) {
    for (const [id, item] of Array.from(model.items)) {
      if (item.turnId === turnId) model.items.delete(id);
    }
    model.turns.delete(turnId);
    // The CEO's disclosure choices belong to an EXCHANGE, and the replacement turn is the
    // same exchange under a new id. Neither set may keep a dead turn id: a stale entry in
    // `collapsed` would silently suppress §6.4's live default the next time that id was
    // reused, and nothing would say why.
    model.expanded.delete(turnId);
    model.collapsed.delete(turnId);
    model.settled.delete(turnId);
    const at = model.turnOrder.indexOf(turnId);
    if (at >= 0) model.turnOrder.splice(at, 1);
    return at;
  }

  /// `rich://turn-status`.
  ///
  /// ## `supersedesTurnId` — A MERGE INSTRUCTION, NOT AN ANNOUNCEMENT
  ///
  /// When the compute lease dies mid-turn, the crashed turn emits `recovering` and the
  /// REPLAY's `queued` carries `supersedesTurnId`. STREAMING.md: *"Without it you would
  /// draw the CEO's single prompt twice."* So the crashed turn's items are removed and the
  /// replacement takes its place IN THE SAME POSITION in the conversation — which is
  /// exactly what a reload does, since `Timeline::project` demotes a superseded turn to
  /// `Internal` and it never appears in a CEO view at all. Live and reload therefore render
  /// the identical exchange.
  ///
  /// Nothing is said to the CEO. §21's calm recovery wording is a product decision that has
  /// not been made, and STREAMING.md is explicit that this must not surface as "we
  /// reconnected". Silence is the choice this slice makes: the work genuinely continued, so
  /// the row keeps reading `Working`.
  ///
  /// ONE VISIBLE ARTIFACT, STATED PLAINLY: the timer restarts. The replacement turn has its
  /// own `startedAt`, and the crashed attempt's span was never recorded — a hard kill
  /// writes no terminal event, so `Turn::active_ms` is `None` for it forever (ledger.rs).
  /// Carrying the older anchor forward would make the live ticker disagree with the frozen
  /// number the same turn reports on completion, and with what a reload shows. The renderer
  /// tracks the turn the ledger measured, and only that one.
  function onTurnStatus(model, p) {
    if (!accepts(model, p)) return { structural: false, rejected: true };
    let structural = false;

    if (p.supersedesTurnId) {
      // THE CEO'S PROMPT IS CARRIED ACROSS, NOT DROPPED WITH THE TURN.
      //
      // Found by running the crash, not by reasoning about it: the optimistic bubble was
      // already adopted onto the CRASHED turn (its `queued` consumed the pending entry), so
      // deleting that turn's items deleted the CEO's sentence, and the replacement's
      // `queued` had no pending bubble left to adopt. The prompt vanished from the
      // conversation until the next snapshot put it back. Re-keying it onto the replacement
      // is what "merge the two turn ids into one exchange" actually means, and it lands the
      // item on `{replacementTurnId}:user` — the SAME id `Timeline::project` derives, so the
      // reload agrees.
      const carried = Object.values(Object.fromEntries(model.items)).find(
        (i) => i.kind === "user_message" && i.turnId === p.supersedesTurnId
      );
      const at = dropTurn(model, p.supersedesTurnId);
      structural = true;
      const t = turnRecord(model, p.turnId);
      t.mergedFrom.push(p.supersedesTurnId);
      if (carried) {
        const realId = `${p.turnId}:user`;
        if (!model.items.has(realId)) {
          model.items.set(realId, Object.assign({}, carried, { id: realId, turnId: p.turnId, pending: false }));
        }
      }
      if (at >= 0) {
        const now = model.turnOrder.indexOf(p.turnId);
        if (now >= 0) model.turnOrder.splice(now, 1);
        model.turnOrder.splice(at, 0, p.turnId);
      }
    }

    const known = model.turns.has(p.turnId);
    const t = turnRecord(model, p.turnId);
    if (!known) structural = true;

    t.status = p.status;
    // Finding 21: `answered` rides on a second `working` status and lasts while the turn is
    // working; any other status ends it.
    t.answered = p.status === "working" && (p.answered === true || t.answered === true);
    if (typeof p.startedAt === "number") t.startedAt = p.startedAt;
    // NEVER `?? 0` and never `now() - startedAt`: `activeDurationMs` is explicitly null
    // until the turn ends, and 0 is a measurement claim (STREAMING.md, §6.3).
    if (typeof p.activeDurationMs === "number") t.activeMs = p.activeDurationMs;
    const wasLive = t.live;
    t.live = p.status === "queued" || p.status === "working" || p.status === "recovering";

    // §6.4's collapse is a TRANSITION, not an instant. A turn that was open only because it
    // was running would otherwise snap shut the moment its terminal status arrived, and the
    // settling collapse `main.js` schedules 180ms later would have nothing left to do.
    // So the live default is CARRIED FORWARD as an explicit entry, and the settle removes
    // it — the same two lines that have always performed the collapse.
    //
    // Skipped when the CEO has already spoken: `collapsed` means he closed it himself, and
    // `settled` means he opened it himself. Neither is overruled here.
    if (wasLive && !t.live && !model.collapsed.has(p.turnId) && !model.settled.has(p.turnId)) {
      model.expanded.add(p.turnId);
    }

    // An answered status is about a turn already running, never the start of the next one.
    if ((p.status === "queued" || p.status === "working") && !p.answered) adoptPendingUserMessage(model, p.turnId);
    if (!t.live && !model.expanded.has(p.turnId)) model.settled.delete(p.turnId);

    return { structural, rejected: false };
  }

  function onMessageStarted(model, p) {
    if (!accepts(model, p)) return { structural: false, rejected: true };
    if (p.visibility && p.visibility !== "ceo") return { structural: false, rejected: true };
    const isNew = putItem(model, {
      kind: "rich_message",
      id: p.messageId,
      entityId: p.entityId,
      threadId: p.threadId,
      turnId: p.turnId,
      createdAt: p.at,
      slot: "stream",
      // Never zero-by-default: a null position means the position was not recorded, and
      // an unpositioned run sorts AFTER every positioned one (timeline.rs `order_key`).
      sequence: typeof p.seq === "number" ? p.seq : null,
      visibility: p.visibility || "ceo",
      phase: p.phase, // "unknown" today, and rendered as such — see the header of this file
      text: model.items.has(p.messageId) ? model.items.get(p.messageId).text : "",
    });
    return { structural: isNew, rejected: false };
  }

  function onMessageDelta(model, p) {
    if (!accepts(model, p)) return { structural: false, rejected: true };
    if (p.visibility && p.visibility !== "ceo") return { structural: false, rejected: true };
    const existing = model.items.get(p.messageId);
    if (!existing) {
      // A delta whose open we missed still renders. The text is durable either way.
      putItem(model, {
        kind: "rich_message",
        id: p.messageId,
        entityId: p.entityId,
        threadId: p.threadId,
        turnId: p.turnId,
        createdAt: p.at,
        slot: "stream",
        sequence: typeof p.seq === "number" ? p.seq : null,
        visibility: "ceo",
        phase: "unknown",
        text: p.textDelta,
      });
      return { structural: true, rejected: false, textOnly: null };
    }
    // REPLACED, NEVER MUTATED. `render` now reuses the DOM of a turn whose items are the
    // same OBJECTS it drew last time, so an item that changes its contents behind that
    // reference would keep a stale node on screen — a delta appended in place, and the
    // prose frozen at whatever the last structural render happened to catch. Every other
    // write in this file already goes through `putItem`, which builds a new object; this
    // was the one exception, and it was the one that fires 52 times a turn.
    model.items.set(p.messageId, Object.assign({}, existing, { text: existing.text + p.textDelta }));
    // Not structural: the caller updates this one node's text in place, so streaming never
    // rebuilds the timeline and never moves focus (§18, §15's "coalesce tiny deltas").
    return { structural: false, rejected: false, textOnly: p.messageId };
  }

  function onMessageCompleted(model, p) {
    if (!accepts(model, p)) return { structural: false, rejected: true };
    if (p.visibility && p.visibility !== "ceo") return { structural: false, rejected: true };
    const existing = model.items.get(p.messageId);
    // `text` is the run's FULL text read back from the ledger — authoritative over anything
    // accumulated from deltas, so a consumer that missed every delta is still correct.
    const isNew = putItem(model, {
      kind: "rich_message",
      id: p.messageId,
      entityId: p.entityId,
      threadId: p.threadId,
      turnId: p.turnId,
      createdAt: existing ? existing.createdAt : p.at,
      slot: "stream",
      sequence: existing ? existing.sequence : null,
      visibility: p.visibility || "ceo",
      phase: p.phase,
      text: p.text,
      closed: true,
    });
    return { structural: isNew, rejected: false, textOnly: isNew ? null : p.messageId };
  }

  /// `rich://activity-upserted`. The payload IS the timeline item a reload projects, plus
  /// `at` — so this is a plain upsert by `id`, last write wins (STREAMING.md). One tool call
  /// is ONE row that arrives several times as it moves `queued -> completed`.
  function onActivityUpserted(model, p) {
    if (!accepts(model, p)) return { structural: false, rejected: true };
    if (p.visibility && p.visibility !== "ceo") return { structural: false, rejected: true };
    const isNew = putItem(model, p);
    return { structural: true, rejected: false, isNew };
  }

  /// `rich://worker-upserted`. The payload IS the `worker_activity` timeline item a reload
  /// projects, plus `at` — so this is the same upsert-by-id as an activity row, through the
  /// same fence and the same visibility gate. One delegated run is ONE row, however many
  /// lifecycle events it produced.
  ///
  /// A separate entry point rather than an alias, because the two events are separate on
  /// the wire and a renderer's subscription list should be the proof of what it draws.
  function onWorkerUpserted(model, p) {
    if (!accepts(model, p)) return { structural: false, rejected: true };
    if (p.visibility && p.visibility !== "ceo") return { structural: false, rejected: true };
    const isNew = putItem(model, p);
    return { structural: true, rejected: false, isNew };
  }

  // ---- §6.4's two defaults -------------------------------------------------------------

  /// Is this turn's work transcript open?
  ///
  /// §6.4 opens with *"While active, the work activity beneath the duration row is expanded
  /// by default"* and goes on to *"collapse the working transcript after a short settling
  /// transition"*. Two DIFFERENT defaults for the same control, chosen by whether the turn
  /// is running — which is why this is a function and not a set lookup. Until this commit
  /// only the second half existed: nothing ever put a live turn into `model.expanded` (that
  /// set is written by the CEO's own toggle alone), so a running turn started closed and the
  /// CEO watched a chevron instead of the work.
  ///
  /// THE CEO ALWAYS WINS, in both directions, and that is what the second set is for. An
  /// explicit close (`collapsed`) beats the live default; an explicit open (`expanded`)
  /// beats the post-completion collapse — `main.js` marks such a turn `settled` so the
  /// settle timer leaves it alone. A turn in NEITHER set has never been touched, and only
  /// then does `live` decide.
  ///
  /// It does not fight the post-completion collapse: when the turn stops being live this
  /// falls back to `expanded`, which the settle has already cleared. No timer is cancelled
  /// and no state is raced.
  /// §6.4 has TWO defaults for an UNTOUCHED turn — expanded while it is active, collapsed
  /// once it settles — and the CEO's own choice overrules both. **Techy mode adds a third
  /// default and overrules neither.**
  ///
  /// A settled turn collapses its work rows, which is right for the calm view: the CEO does
  /// not need "Read 3 files" on screen forever. It is exactly wrong for techy mode, where
  /// the machinery IS what he turned on, and a technical view that opens with every turn
  /// collapsed would show him nothing he asked for and make him click once per turn to
  /// undo his own setting. So an untouched turn is expanded while the mode is on.
  ///
  /// His explicit choice still wins in both directions — a turn he collapsed by hand stays
  /// collapsed here, because `collapsed` is checked first and this only changes the default
  /// for a turn nobody has touched.
  function isTurnExpanded(model, turnId) {
    if (model.expanded.has(turnId)) return true;
    if (model.collapsed.has(turnId)) return false;
    if (model.technical) return true;
    const t = model.turns.get(turnId);
    return !!(t && t.live);
  }

  /// The CEO's own toggle. Records the choice EXPLICITLY — in `expanded` or in `collapsed`
  /// — so it survives the turn ending, and marks the turn `settled` so the post-completion
  /// collapse does not overrule a deliberate open.
  function toggleTurn(model, turnId) {
    const open = isTurnExpanded(model, turnId);
    if (open) {
      model.expanded.delete(turnId);
      model.collapsed.add(turnId);
    } else {
      model.collapsed.delete(turnId);
      model.expanded.add(turnId);
    }
    model.settled.add(turnId);
    return !open;
  }

  /// Whether ONE tool call's raw pane is open (techy mode §3.4).
  ///
  /// Default CLOSED, with no per-turn or per-liveness subtlety: unlike §6.4's work
  /// transcript, there is no state in which the CEO is better served by every raw payload
  /// on screen at once. A single Bash result can be 32 KB.
  function isMachineryExpanded(model, machineryId) {
    return model.expandedMachinery.has(machineryId);
  }

  function toggleMachinery(model, machineryId) {
    const open = isMachineryExpanded(model, machineryId);
    if (open) model.expandedMachinery.delete(machineryId);
    else model.expandedMachinery.add(machineryId);
    return !open;
  }

  // ---- ordering ------------------------------------------------------------------------

  /// `(turn, slot, sequence)` — the same key `TimelineBase::order_key` uses, with the turn
  /// order carried by first appearance. An UNPOSITIONED stream item sorts after every
  /// positioned one, because claiming it came first would be a claim (timeline.rs).
  /// `turnIndex` is a `turnId -> position` map built once per projection. It used to be
  /// `model.turnOrder.indexOf(...)`, evaluated inside a sort comparator — O(turns) per
  /// comparison, on every structural render. The KEY is unchanged; only the lookup is.
  function orderKey(model, item, turnIndex) {
    const idx = turnIndex ? turnIndex.get(item.turnId) : model.turnOrder.indexOf(item.turnId);
    const turnIdx = idx === undefined || idx === null || idx < 0 ? -1 : idx;
    const slot = SLOT_RANK[item.slot] !== undefined ? SLOT_RANK[item.slot] : 1;
    const positioned = typeof item.sequence === "number" ? 0 : 1;
    const seq = typeof item.sequence === "number" ? item.sequence : 0;
    return [turnIdx < 0 ? model.turnOrder.length : turnIdx, slot, positioned, seq];
  }

  function cmpKey(a, b) {
    for (let i = 0; i < a.length; i++) {
      if (a[i] !== b[i]) return a[i] - b[i];
    }
    return 0;
  }

  /// The model grouped into the shape §5/§6 renders: one entry per turn, with the CEO's
  /// message, the duration row and the ordered stream lane.
  function turnsOf(model) {
    const byTurn = new Map();
    for (const item of model.items.values()) {
      const list = byTurn.get(item.turnId) || [];
      list.push(item);
      byTurn.set(item.turnId, list);
    }
    const turnIndex = new Map();
    const ids = model.turnOrder.slice();
    for (let i = 0; i < ids.length; i++) turnIndex.set(ids[i], i);
    for (const id of byTurn.keys()) {
      if (turnIndex.has(id)) continue;
      turnIndex.set(id, ids.length);
      ids.push(id);
    }

    // §9.2's "Added while Rich was working" cue, DERIVED rather than flagged.
    //
    // Nothing on the wire says "this message was steering". The obvious fix — have the
    // sender set a flag — makes the cue a property of THIS SESSION: it would show while
    // the CEO watched and vanish on the next reload, which is exactly the live-vs-reload
    // disagreement the timeline is built to avoid.
    //
    // So it is computed from durable, measured timestamps that are already on the wire: a
    // CEO message whose `createdAt` falls inside ANOTHER turn's active span was, by
    // definition, added while Rich was working. True by construction, identical live and
    // after a restart, and it needs no new field anywhere. A turn's own span never
    // qualifies — `PromptReceived` is always written before `TurnStarted`.
    const spans = [];
    for (const [id, t] of model.turns) {
      if (typeof t.startedAt !== "number") continue;
      const end = typeof t.activeMs === "number" ? t.startedAt + t.activeMs : t.live ? Infinity : null;
      if (end === null) continue; // ended, but when was never recorded — claims nothing
      spans.push({ id, from: t.startedAt, to: end });
    }
    // The predicate above reads `spans.some(...)`, evaluated once per turn over every
    // turn's span. That is quadratic, and it cost 249 ms of projection on a 10,000-turn
    // thread — on EVERY structural render, i.e. once per activity row while Rich works.
    //
    // The same answer in O(log turns), and it is the EXCLUSION that makes it interesting.
    // Spans sorted by `from`: every span that could contain a timestamp `t` starts at or
    // before it, so the candidates are a prefix, and "does any candidate reach `t`" is a
    // running maximum of `to`. But a turn's OWN span always reaches its own prompt, so a
    // plain maximum answers "yes" for every message and skips nothing — the first version
    // of this was exactly that, and measured identical to the scan it replaced.
    //
    // So the running maximum keeps its TOP TWO, and their ids. Span ids are turn ids and
    // are unique, so the runner-up is by construction a different turn: if the best
    // candidate is the message's own turn, the second-best decides, and otherwise the best
    // does. Exactly the old predicate, never an approximation of it — `scale.js` check 7
    // runs both over a scripted mix of overlapping, nested, live and non-steering turns and
    // requires the two to agree message for message.
    const sorted = spans.slice().sort((a, b) => a.from - b.from);
    const bestTo = new Array(sorted.length);
    const bestId = new Array(sorted.length);
    const nextTo = new Array(sorted.length);
    let b = -Infinity;
    let bid = null;
    let n = -Infinity;
    for (let i = 0; i < sorted.length; i++) {
      const sp = sorted[i];
      if (sp.to > b) {
        n = b;
        b = sp.to;
        bid = sp.id;
      } else if (sp.to > n) {
        n = sp.to;
      }
      bestTo[i] = b;
      bestId[i] = bid;
      nextTo[i] = n;
    }
    const addedWhileWorking = (item) => {
      const t = item.createdAt;
      if (typeof t !== "number") return false;
      let lo = 0;
      let hi = sorted.length - 1;
      let at = -1;
      while (lo <= hi) {
        const mid = (lo + hi) >> 1;
        if (sorted[mid].from <= t) {
          at = mid;
          lo = mid + 1;
        } else {
          hi = mid - 1;
        }
      }
      if (at < 0) return false;
      return bestId[at] === item.turnId ? nextTo[at] >= t : bestTo[at] >= t;
    };

    return ids
      .filter((id) => byTurn.has(id) || model.turns.has(id))
      .map((turnId) => {
        const items = (byTurn.get(turnId) || [])
          .slice()
          .sort((a, b) => cmpKey(orderKey(model, a, turnIndex), orderKey(model, b, turnIndex)));
        return {
          turnId,
          record: model.turns.get(turnId) || null,
          user: (() => {
            const u = items.find((i) => i.kind === "user_message") || null;
            return u ? Object.assign({}, u, { steering: addedWhileWorking(u) }) : null;
          })(),
          // Everything the lease emitted, in shared-counter order — prose and activity
          // INTERLEAVED, which is what makes §25's "commentary is restored in its original
          // order" true when the transcript is expanded: prose rows never move, activity
          // rows appear between them.
          stream: items.filter((i) => RENDERED_STREAM_KINDS.indexOf(i.kind) >= 0),
          // The files Rich put in front of the user in this turn (round 36 screen 3). Their
          // own slot, like `user`, because they are drawn UNDER his words whenever in the
          // turn the tool was called — never between two runs of his prose.
          files: items.filter((i) => i.kind === "rich_file"),
          // Rich's save of the user's instructions (round 36 screen 1): the "Your instructions ·
          // Saved" card, drawn under his words like the files.
          instructions: items.filter((i) => i.kind === "instructions_saved"),
          // The turn died to the model API (richos-core `upstream.rs`, open-items row
          // 3.30). Its own slot, like `user`, because it is drawn INSIDE the failure card
          // rather than in the stream — the card is where a reader already looks when a
          // turn ends badly, and a second place to look is a second thing to miss.
          outage: items.find((i) => i.kind === "upstream_outage") || null,
          // WHY an ordinary turn ended without finishing (richos-core `interruption.rs`,
          // the 2026-09-17 nightly's D2). Its own slot for the same reason `outage` has
          // one: it is drawn inside the failure card, not in the stream.
          interruption: items.find((i) => i.kind === "turn_interruption") || null,
          // NOT DROPPED SILENTLY — see RENDERED_STREAM_KINDS. `user_message`,
          // `work_duration`, `upstream_outage` and `turn_interruption` are excluded
          // because they have their OWN render slots above; what lands here is genuinely
          // undrawn.
          unrendered: items.filter(
            (i) =>
              RENDERED_STREAM_KINDS.indexOf(i.kind) < 0 &&
              i.kind !== "user_message" &&
              i.kind !== "rich_file" &&
              i.kind !== "instructions_saved" &&
              i.kind !== "work_duration" &&
              i.kind !== "upstream_outage" &&
              i.kind !== "turn_interruption"
          ),
        };
      });
  }

  const isProse = (i) => i.kind === "rich_message";
  /// A status line from the app rather than something Rich said (finding #8). Drawn in the
  /// same lane, in the same order, and never with Rich's identity on it.
  const isLocalNotice = (i) => i.kind === "local_notice";
  const isActivity = (i) => i.kind === "activity";
  const isWorker = (i) => i.kind === "worker_activity";
  /// EVERYTHING POSITIVELY TYPED AS WORK. This is the set §6.4 collapses — "This includes
  /// interim Rich commentary, semantic activity, plans and WORKER LIFECYCLE ROWS" — and it
  /// is why a worker row belongs in the disclosure lane rather than beside the prose: the
  /// row is work, and the collapse hides work. Prose is still never hidden (see the header).
  const isWork = (i) => isActivity(i) || isWorker(i);

  /// THE KINDS THIS SLICE DRAWS. Everything else in the payload is kept in the model and
  /// reported on `turn.unrendered` rather than quietly discarded, because a silent drop is
  /// how a missing row stops being noticeable.
  ///
  /// `worker_activity` JOINED THIS LIST ON 2026-08-29 AND THAT WAS A LIVE REGRESSION FIX.
  /// Slice 2b made a `Task` call with an extractable `agentId` project as
  /// `kind: "worker_activity"` at `ceo` visibility instead of as an ordinary `activity`
  /// row; slice 5 drew nothing for that kind and said so here rather than dropping it
  /// silently. Neither was wrong alone, and together they meant a turn in which Rich
  /// delegated work showed the delegation NOWHERE in the CEO's timeline. The producer half
  /// was wired in the same branch (`Spine::set_worker_events`) — before that, the app could
  /// not emit this kind at all and a delegation reached the CEO as one nameless activity
  /// row reading "Worked".
  const RENDERED_STREAM_KINDS = ["question", "rich_message", "local_notice", "activity", "worker_activity"];

  // -------------------------------------------------------------------------------------
  // THE RENDER
  // -------------------------------------------------------------------------------------

  const CEO_BUBBLE_LINE_CLAMP = 18; // §5.1: "show the first 16 to 20 lines"

  function elem(tag, cls, text) {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = text;
    return n;
  }

  function srOnly(text) {
    return elem("span", "sr-only", text);
  }

  function formatClock(ms) {
    return new Date(ms).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
  }

  /// §5.1 — the CEO's message. Right aligned, a QUIET highlighted surface (§0.1: "must not
  /// use a saturated consumer-chat color"), capped, and visually lighter than Rich's prose.
  ///
  /// NO EDIT AFFORDANCE. §5.1/§25 want Edit to start a superseding branch that leaves the
  /// original in the evidence log. Nothing in this build can branch a turn, and a button
  /// that silently rewrote evidence would be the exact failure §5.1 forbids. Copy ships;
  /// Edit is absent and reported as absent.
  function renderUserMessage(item, opts) {
    const art = elem("article", "tl-user");
    art.appendChild(srOnly("You said"));

    const bubble = elem("div", "tl-user-bubble");
    const body = elem("div", "tl-user-text", item.text);
    bubble.appendChild(body);

    const lines = item.text.split("\n").length;
    const longEnough = lines > CEO_BUBBLE_LINE_CLAMP || item.text.length > 1400;
    if (longEnough) {
      const expanded = opts.expandedMessages.has(item.id);
      if (!expanded) bubble.classList.add("is-clamped");
      const more = elem("button", "tl-more", expanded ? "Show less" : "Show more");
      more.type = "button";
      more.id = "more:" + item.id;
      more.setAttribute("aria-expanded", expanded ? "true" : "false");
      more.addEventListener("click", () => {
        if (expanded) opts.expandedMessages.delete(item.id);
        else opts.expandedMessages.add(item.id);
        opts.rerender();
      });
      bubble.appendChild(more);
    }
    art.appendChild(bubble);

    // §5.1: "Place timestamp and message actions just beneath the bubble's lower-right
    // edge. Revealing them must not change the bubble width or move surrounding content."
    // The row is always in the layout and only its OPACITY changes — so nothing reflows.
    const actions = elem("div", "tl-user-actions");
    const stamp = elem("span", "tl-stamp", formatClock(item.createdAt));
    actions.appendChild(stamp);
    const copy = elem("button", "tl-mini-btn", "Copy");
    copy.type = "button";
    copy.id = "copy:" + item.id;
    copy.setAttribute("aria-label", "Copy your message");
    copy.addEventListener("click", () => opts.copy(item.text, copy));
    actions.appendChild(copy);
    art.appendChild(actions);

    // §9.2: "Steering messages render as CEO bubbles with a small `Added while Rich was
    // working` cue." Placed in the actions row beside the timestamp so it never changes the
    // bubble's width — §5.1's rule that revealing message furniture must not reflow.
    if (item.steering) {
      // Its OWN row, not the actions row. The actions row is `opacity: 0` until hover
      // (§5.1's no-reflow rule), and a cue that only exists on hover is not a cue — the CEO
      // would have to already suspect the thing it is there to tell him.
      const cue = elem("p", "tl-steer-cue", "Added while Rich was working");
      art.insertBefore(cue, actions);
      art.classList.add("is-steering");
    }

    if (item.pending) art.classList.add("is-pending");
    return art;
  }

  // -------------------------------------------------------------------------------------
  // MARKDOWN (§5.4) — a deliberately small subset, built as DOM
  // -------------------------------------------------------------------------------------
  //
  // WHAT WAS WRONG. The engine emits Markdown and this surface has never rendered it, so on
  // the published v1.0.1 the first answer a customer ever received read, on screen, as:
  //
  //     **1. Tell me about Lakeside Advisory.**
  //     **2. Authorize the connectors.**
  //
  // Three such lines in one answer. Nothing was broken — `renderRichMessage` set
  // `textContent` and the asterisks are simply what the model wrote.
  //
  // THE SUBSET IS THE ONE RICH ACTUALLY EMITS, AND NOTHING MORE: `**bold**`, `*italic*`,
  // `` `code` ``, ordered and unordered lists, headings, and blank-line paragraph breaks.
  // No tables, no links, no images, no HTML passthrough, no library and no CDN. A renderer
  // that accepts more than the product emits is a larger attack surface bought with nothing.
  //
  // NO STRING EVER BECOMES HTML, ANYWHERE ON THIS PATH. Every leaf below is a text node and
  // every container is `createElement`; `innerHTML` is not used, so there is no escaping
  // step that could be forgotten or ordered wrongly — model output cannot become markup
  // because it is never parsed as markup. `markdown.js` in the suite drives
  // `<img src=x onerror=alert(1)>` through the real renderer and asserts both that the
  // characters are on screen and that `querySelectorAll("img")` finds nothing.
  //
  // AN UNMATCHED MARKER IS LITERAL. `**` with no closer, a lone backtick, a `*` used as
  // punctuation: each falls through to its own characters and the rest of the message is
  // unaffected. There is no path here that can swallow the tail of an answer, which is the
  // failure mode that would matter more than any formatting.

  const MD_HEADING = /^[ \t]*(#{1,6})[ \t]+(.*?)[ \t]*#*[ \t]*$/;
  // A bullet needs whitespace after its marker, which is what keeps `**bold on its own
  // line**` — the exact shape that produced the defect above — out of the list branch.
  const MD_BULLET = /^[ \t]*[-*+][ \t]+(.*)$/;
  const MD_ORDERED = /^[ \t]*(\d{1,9})[.)][ \t]+(.*)$/;

  /// Where a `*` or `**` run closes, or -1. A `**` never closes a `*`, so `*a**b*` keeps its
  /// middle pair literal rather than tearing the emphasis in half.
  ///
  /// A CLOSER MAY NOT BE PRECEDED BY WHITESPACE — Markdown's right-flanking rule, and it is
  /// here because of a real failure rather than for completeness. Without it `3 * 4 * 5`
  /// rendered as `3  4  5`: the two asterisks paired up, the span swallowed them, and the
  /// CEO's arithmetic quietly lost two characters. The matching left-flanking half is at the
  /// call site.
  function markdownClose(text, from, marker) {
    for (let j = from; j + marker.length <= text.length; j += 1) {
      if (text[j] !== "*") continue;
      const isDouble = text[j + 1] === "*";
      if (marker === "**") {
        if (!isDouble) continue;
      } else if (isDouble) {
        j += 1; // a `**` is not a closer for a `*`
        continue;
      }
      if (/\s/.test(text[j - 1])) {
        if (marker === "**") j += 1;
        continue;
      }
      return j;
    }
    return -1;
  }

  // ---- A FILE RICH NAMES IS A LINK TO IT (output side-panel PRD §6.5) ------------------------
  //
  // A code span whose text is a file the thread's OUTPUT RECORD holds — its path, its `~/` path,
  // or its name when no other recorded file shares it — renders as round 17's file link and
  // opens the Output panel on that file. Only recorded files become links: a name Rich mentions
  // that nothing witnessed stays a code span, which is honest. The link READS the record
  // (`window.RichOutput.links`, handed in as `opts.output`) and never feeds it.
  //
  // `outputLinks` is the module's copy of the last `opts.output` `render` was given, so the
  // streaming path (`updateProse`, which has no `opts`) links the same names the structural
  // path does. Clicks resolve through it at click time, so a reused node never calls a stale
  // closure (the contract over TURN REUSE below).
  let outputLinks = null;

  const SVG_NS = "http://www.w3.org/2000/svg";

  function fileGlyph() {
    const svg = document.createElementNS(SVG_NS, "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("fill", "none");
    svg.setAttribute("stroke", "currentColor");
    svg.setAttribute("stroke-width", "2");
    svg.setAttribute("stroke-linecap", "round");
    svg.setAttribute("stroke-linejoin", "round");
    svg.setAttribute("aria-hidden", "true");
    svg.setAttribute("focusable", "false");
    for (const d of ["M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z", "M14 3v5h5"]) {
      const p = document.createElementNS(SVG_NS, "path");
      p.setAttribute("d", d);
      svg.appendChild(p);
    }
    return svg;
  }

  /// One file link: the glyph and `label` (what Rich wrote, or the file's name in the produced
  /// strip). The output id rides on the node; the click asks the panel to open on it.
  function fileLink(entry, label) {
    const b = elem("button", "tl-file-link");
    b.type = "button";
    b.dataset.outputId = entry.id;
    b.appendChild(fileGlyph());
    b.appendChild(elem("span", null, label));
    b.addEventListener("click", () => {
      if (outputLinks && outputLinks.openFile) outputLinks.openFile(entry.id, b);
    });
    return b;
  }

  /// Inline spans, appended into `into` as DOM. Recurses on the INSIDE of a span, which is
  /// always strictly shorter than what it was called with, so it terminates. `links` is the
  /// output record's lookup (§6.5) or nothing; without it a code span is always a code span.
  /// `doc` is the document extension (below `renderMarkdownInto`): a FILE in the Output panel,
  /// never Rich's answer. It adds backslash escapes and `[label](target)` drawn as its label.
  /// WHAT CLAUDE WROTE, AS IT IS DRAWN (CEO, 2026-10-19): with the comma before and/or/nor deleted
  /// while the Oxford comma setting is off, his default, and untouched while it is on. The filter
  /// is `oxford-comma.js` and it is applied HERE, where Rich's and his team's words are drawn,
  /// never to the stored text: `Copy` still copies what Rich wrote. `renderMarkdownInto` itself
  /// filters nothing, so the app's own lines and a file in the Output panel are drawn as written.
  function said(text) {
    return window.RichOxford ? window.RichOxford.display(text) : text;
  }

  function markdownInline(text, into, links, doc) {
    let plain = "";
    const flush = () => {
      if (!plain) return;
      into.appendChild(document.createTextNode(plain));
      plain = "";
    };
    let i = 0;
    while (i < text.length) {
      const c = text[i];
      if (doc && c === "\\" && MD_ESCAPABLE.test(text[i + 1] || "")) {
        plain += text[i + 1];
        i += 2;
        continue;
      }
      if (doc && (c === "[" || (c === "!" && text[i + 1] === "["))) {
        const link = mdLinkAt(text, c === "!" ? i + 1 : i);
        if (link) {
          flush();
          // §7: "links render as their text (no navigation out of the panel)". The label is
          // drawn and the target is dropped — there is no `href`, so nothing here can leave
          // the panel. An image is its alt text, said as one.
          const span = elem("span", c === "!" ? "tl-md-alt" : "tl-md-link");
          markdownInline(link.label, span, null, doc);
          into.appendChild(span);
          i = link.end;
          continue;
        }
      }
      if (c === "`") {
        const end = text.indexOf("`", i + 1);
        if (end > i + 1) {
          flush();
          const code = text.slice(i + 1, end);
          const hit = links && links.forText ? links.forText(code) : null;
          into.appendChild(hit ? fileLink(hit, code) : elem("code", "tl-md-code", code));
          i = end + 1;
          continue;
        }
      } else if (c === "*") {
        const strong = text.slice(i, i + 2) === "**";
        const marker = strong ? "**" : "*";
        // LEFT-FLANKING: an opener is not an opener when the next character is whitespace or
        // absent. This is the half of the `3 * 4 * 5` fix that stops the FIRST asterisk from
        // ever starting a span; `markdownClose` holds the other half.
        const next = text[i + marker.length];
        const end = next && !/\s/.test(next) ? markdownClose(text, i + marker.length, marker) : -1;
        // `end > i + marker.length` rejects an EMPTY span, so `**`, `****` and `* *` stay
        // literal rather than producing an invisible element.
        if (end > i + marker.length) {
          flush();
          const node = elem(strong ? "strong" : "em", strong ? "tl-md-strong" : "tl-md-em");
          markdownInline(text.slice(i + marker.length, end), node, links, doc);
          into.appendChild(node);
          i = end + marker.length;
          continue;
        }
      }
      plain += c;
      i += 1;
    }
    flush();
  }

  /// Blocks. `root` is emptied and rebuilt.
  ///
  /// A paragraph keeps its OWN newlines as characters: `.tl-prose` is `white-space:
  /// pre-wrap`, and it has rendered a single newline as a line break since this surface
  /// existed. Consuming them here would silently reflow every answer that has ever been
  /// sent, so the only newlines this function eats are the blank lines BETWEEN blocks and
  /// the ones inside a list.
  ///
  /// `links` (optional) is the output record's lookup, so a code span naming a recorded file is
  /// a link to it (§6.5). Everything else about the subset is unchanged by it.
  ///
  /// THE BLOCKS BELOW (tables, quotations, rules, fenced code) are drawn for every caller: Rich
  /// answers with tables, and on nightly 64 a stopped answer's table read as raw `| ... |` lines
  /// (Ray, s5). `opts.document` (a file previewed in the Output panel) adds only INLINE things:
  /// backslash escapes and `[label](target)` drawn as its label.
  function renderMarkdownInto(root, text, links, opts) {
    root.textContent = "";
    const doc = !!(opts && opts.document);
    const lines = String(text == null ? "" : text).split("\n");
    let i = 0;
    while (i < lines.length) {
      const line = lines[i];
      if (!line.trim()) {
        i += 1;
        continue;
      }

      const block = mdDocumentBlock(lines, i, links, opts);
      if (block) {
        root.appendChild(block.node);
        i = block.next;
        continue;
      }

      const heading = MD_HEADING.exec(line);
      if (heading) {
        // `role`/`aria-level` rather than `h1`–`h6`: the type scale (§17.2) is set on
        // `.tl-prose` and an unstyled `h1` inside it would be a 32px shout in the middle of
        // an answer. The semantics a screen reader needs are carried either way.
        const node = elem("div", "tl-md-h");
        node.setAttribute("role", "heading");
        node.setAttribute("aria-level", String(Math.min(6, heading[1].length + 2)));
        markdownInline(heading[2], node, links, doc);
        root.appendChild(node);
        i += 1;
        continue;
      }

      const ordered = MD_ORDERED.exec(line);
      const bullet = ordered ? null : MD_BULLET.exec(line);
      if (ordered || bullet) {
        const list = elem(ordered ? "ol" : "ul", "tl-md-list");
        if (ordered && ordered[1] !== "1") list.setAttribute("start", ordered[1]);
        while (i < lines.length) {
          if (MD_RULE.test(lines[i])) break;
          const m = ordered ? MD_ORDERED.exec(lines[i]) : MD_BULLET.exec(lines[i]);
          if (!m) break;
          const li = elem("li", "tl-md-item");
          markdownInline(ordered ? m[2] : m[1], li, links, doc);
          list.appendChild(li);
          i += 1;
        }
        root.appendChild(list);
        continue;
      }

      const para = elem("div", "tl-md-p");
      const buf = [];
      while (
        i < lines.length &&
        lines[i].trim() &&
        !MD_HEADING.test(lines[i]) &&
        !MD_ORDERED.test(lines[i]) &&
        !MD_BULLET.test(lines[i]) &&
        !mdStartsDocumentBlock(lines, i)
      ) {
        buf.push(lines[i]);
        i += 1;
      }
      markdownInline(buf.join("\n"), para, links, doc);
      root.appendChild(para);
    }
  }

  // ---- THE DOCUMENT EXTENSION (output side-panel PRD §7, slice S5) ---------------------------
  //
  // A Markdown FILE in the Output panel is a document: Rich writes briefs and summaries with
  // tables, quotations, rules and fenced code, and §7 asks for "the app's DOM-only renderer
  // extended with tables, blockquotes, horizontal rules and fenced code; links render as their
  // text (no navigation out of the panel)". Rich's ANSWERS use the same four blocks, because he
  // writes tables there too (nightly 64 s5: a stopped answer's table read as raw `|` lines).
  // Backslash escapes and `[label](target)` stay ONLY under `opts.document`, so a link in an
  // answer still reads as written; `markdown.js` checks both.
  //
  // THE SAME GUARANTEE HOLDS: every leaf is a text node, every container `createElement`. A
  // fenced block's contents are one text node inside `<pre><code>`, a table cell is inline
  // Markdown through `markdownInline`, a quotation's inside is this renderer again. A link has
  // no `href` — there is no element here that can navigate — and an image is its alt text.
  //
  // UNCLOSED IS LITERAL OR RUNS TO THE END, NEVER LOST: a fence with no closer holds the rest of
  // the file as code (CommonMark's rule, and every character is still on screen); a `|` line
  // with no delimiter row under it is a paragraph.

  const MD_ESCAPABLE = /^[!-/:-@[-`{-~]$/;
  // Linear on any line: the info word is one run of non-space, and the rest is not captured.
  const MD_FENCE_OPEN = /^[ \t]{0,3}(`{3,}|~{3,})[ \t]*([^`\s]*)[^`]*$/;
  const MD_RULE = /^[ \t]{0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$/;
  const MD_QUOTE = /^[ \t]{0,3}>[ \t]?(.*)$/;
  const MD_TABLE_DELIM = /^[ \t]*\|?(?:[ \t]*:?-+:?[ \t]*\|)*[ \t]*:?-+:?[ \t]*\|?[ \t]*$/;

  /// `[label](target)` starting at `at` (the `[`): the label and where the link ends, or null.
  /// The label may not hold another `[` or `]`; the target may not hold a space-free `)`.
  function mdLinkAt(text, at) {
    if (text[at] !== "[") return null;
    const close = text.indexOf("]", at + 1);
    if (close < 0 || text[close + 1] !== "(") return null;
    const label = text.slice(at + 1, close);
    if (label.indexOf("[") >= 0) return null;
    const end = text.indexOf(")", close + 2);
    if (end < 0 || /\s/.test(text.slice(close + 2, end).trim()) || end === close + 2) return null;
    return { label, end: end + 1 };
  }

  /// A table row's cells: outer pipes dropped, split on every `|` not escaped as `\|`.
  function mdCells(line) {
    let t = line.trim();
    if (t.startsWith("|")) t = t.slice(1);
    if (t.endsWith("|") && !t.endsWith("\\|")) t = t.slice(0, -1);
    const cells = [];
    let cur = "";
    for (let k = 0; k < t.length; k += 1) {
      if (t[k] === "\\" && t[k + 1] === "|") {
        cur += "|";
        k += 1;
      } else if (t[k] === "|") {
        cells.push(cur.trim());
        cur = "";
      } else {
        cur += t[k];
      }
    }
    cells.push(cur.trim());
    return cells;
  }

  /// A GFM table starts here: a line with a `|`, and under it a delimiter row with as many cells.
  function mdTableAt(lines, i) {
    const head = lines[i];
    const delim = lines[i + 1];
    if (head.indexOf("|") < 0 || delim == null || !MD_TABLE_DELIM.test(delim) || delim.indexOf("-") < 0) return false;
    return mdCells(head).length === mdCells(delim).length;
  }

  function mdStartsDocumentBlock(lines, i) {
    const line = lines[i];
    return MD_FENCE_OPEN.test(line) || MD_RULE.test(line) || MD_QUOTE.test(line) || mdTableAt(lines, i);
  }

  /// One document block starting at `lines[i]`, or null when none does: `{ node, next }`.
  /// `links` and `opts` are the caller's, so a cell or a quotation reads like the text around it.
  function mdDocumentBlock(lines, i, links, opts) {
    const doc = !!(opts && opts.document);
    const line = lines[i];

    const fence = MD_FENCE_OPEN.exec(line);
    if (fence) {
      const mark = fence[1];
      const closer = new RegExp("^[ \\t]{0,3}" + (mark[0] === "`" ? "`" : "~") + "{" + mark.length + ",}[ \\t]*$");
      const body = [];
      let k = i + 1;
      while (k < lines.length && !closer.test(lines[k])) {
        body.push(lines[k]);
        k += 1;
      }
      const pre = elem("pre", "tl-md-pre");
      const code = elem("code", "tl-md-fence", body.join("\n"));
      if (fence[2]) code.dataset.lang = fence[2].split(/\s+/)[0];
      pre.appendChild(code);
      return { node: pre, next: Math.min(lines.length, k + 1) };
    }

    if (MD_RULE.test(line)) {
      return { node: elem("hr", "tl-md-rule"), next: i + 1 };
    }

    if (MD_QUOTE.test(line)) {
      const inner = [];
      let k = i;
      let m;
      while (k < lines.length && (m = MD_QUOTE.exec(lines[k]))) {
        inner.push(m[1]);
        k += 1;
      }
      const quote = elem("blockquote", "tl-md-quote");
      // Each line lost its `>`, so the inside is strictly shorter: the recursion ends.
      renderMarkdownInto(quote, inner.join("\n"), links, opts);
      return { node: quote, next: k };
    }

    if (mdTableAt(lines, i)) {
      const head = mdCells(line);
      const align = mdCells(lines[i + 1]).map((c) =>
        c.startsWith(":") && c.endsWith(":") ? "center" : c.endsWith(":") ? "right" : ""
      );
      const wrap = elem("div", "tl-md-table-wrap");
      const table = elem("table", "tl-md-table");
      const thead = elem("thead");
      const tr = elem("tr");
      head.forEach((c, n) => {
        const th = elem("th");
        if (align[n]) th.style.textAlign = align[n];
        markdownInline(c, th, links, doc);
        tr.appendChild(th);
      });
      thead.appendChild(tr);
      table.appendChild(thead);
      const tbody = elem("tbody");
      let k = i + 2;
      // A quotation ends the table even with no blank line before it, as a fence does: a `>`
      // line holding a `|` is still a quotation (review rv-20261018T195734Z-d14f5d12-fa08#1).
      while (k < lines.length && lines[k].trim() && lines[k].indexOf("|") >= 0 && !MD_FENCE_OPEN.test(lines[k]) && !MD_QUOTE.test(lines[k])) {
        const cells = mdCells(lines[k]);
        const row = elem("tr");
        for (let n = 0; n < head.length; n += 1) {
          const td = elem("td");
          if (align[n]) td.style.textAlign = align[n];
          markdownInline(cells[n] || "", td, links, doc);
          row.appendChild(td);
        }
        tbody.appendChild(row);
        k += 1;
      }
      table.appendChild(tbody);
      wrap.appendChild(table);
      return { node: wrap, next: k };
    }

    return null;
  }

  /// §5.2/§5.4 — Rich's prose. ONE treatment for every run, because `phase` is unknown.
  /// See the header of this file before adding a second one.
  function renderRichMessage(item, opts) {
    const art = elem("article", "tl-rich");
    art.dataset.messageId = item.id;
    if (item.phase === "proactive") art.classList.add("tl-rich--proactive");
    art.appendChild(srOnly(item.phase === "proactive" ? "Rich reached out" : "Rich said"));

    // §5.2: "left aligned with RICH'S IDENTITY" — one identity per turn, not one per run.
    //
    // A turn where Rich talks, runs a tool and talks again is TWO messages (one per
    // contiguous run of the shared seq counter), and stamping "Rich" on each of them made
    // the lane read like two speakers. §6.1 says the same thing about the duration row: "It
    // does not need to repeat `Rich` in the text." Observed in the WebKit run before this
    // change — the name appeared above every run of a single turn.
    if (opts.showIdentity()) {
      const meta = elem("div", "tl-rich-meta");
      if (opts.showAvatar()) {
        const avatar = document.createElement("img");
        avatar.className = "tl-avatar";
        avatar.src = "assets/rich-hand.png";
        avatar.alt = "";
        meta.appendChild(avatar);
      }
      meta.appendChild(elem("span", "tl-who", "Rich"));
      if (item.phase === "proactive") meta.appendChild(elem("span", "tl-whisper", "reached out"));
      art.appendChild(meta);
    }

    // §5.4's Markdown, and STILL no HTML path: `renderMarkdownInto` builds DOM nodes and
    // text nodes only. `item.text` is the model's text, unchanged and untrusted, and the
    // `Copy` button below deliberately keeps copying THAT rather than what is on screen —
    // what he pastes should be what Rich wrote, markers and all.
    const body = elem("div", "tl-prose");
    body.id = "prose:" + item.id;
    renderMarkdownInto(body, said(item.text), opts.output);
    if (!item.closed && item.text) body.classList.add("is-streaming");
    art.appendChild(body);

    // ROUND 18's BUTTONS UNDER RICH'S LINES ABOUT CLAUDE ACCOUNTS (the CEO 2026-10-06, feedback
    // item 9): "See your accounts" under the switch line, "Add a second account" and "Not now"
    // under his one-time suggestion. The back end records which proactive turns those are
    // (claude_accounts.rs `noted`); `accounts.js` fills the slot, and leaves it empty (and
    // undrawn) for every other line. Only Rich's proactive lines carry one.
    if (item.phase === "proactive" && window.RichAccounts && window.RichAccounts.noteSlot) {
      const slot = window.RichAccounts.noteSlot(item.turnId);
      if (slot) art.appendChild(slot);
    }

    const actions = elem("div", "tl-rich-actions");
    const copy = elem("button", "tl-mini-btn", "Copy");
    copy.type = "button";
    copy.id = "copy:" + item.id;
    copy.setAttribute("aria-label", "Copy Rich's message");
    copy.addEventListener("click", () => opts.copy(item.text, copy));
    actions.appendChild(copy);
    art.appendChild(actions);
    return art;
  }

  /// **A STATUS LINE FROM THE APP, AND EVERY DIFFERENCE FROM A MESSAGE IS DELIBERATE**
  /// (Ray's candidate-.4 finding #8).
  ///
  /// - **No byline and no avatar.** The defect was that it had them: a voice notice carried
  ///   Rich's identity, so reading back, nothing distinguished the app reporting on itself
  ///   from Rich answering.
  /// - **No Copy button.** Copy exists to hand him what Rich wrote. There is nothing here
  ///   that Rich wrote.
  /// - **`<aside role="note">`, the same object the failure card already is**, because this
  ///   is the same category of thing: the app telling him about a state he did not cause,
  ///   beside the conversation rather than inside it.
  /// - **A screen reader is told what it is**, in the slot where the message lane says
  ///   *"Rich said"*.
  ///
  /// Contrast, computed under WebKit in both themes rather than eyeballed, and re-measured
  /// by `ui/tests/local-notice.js` on every run: the body is `--ink` on `--surface`, and the
  /// left rule — a non-text indicator, floor 3:1 — is `--attention` on the same.
  function renderLocalNotice(item) {
    const card = elem("aside", "tl-notice");
    card.dataset.messageId = item.id;
    card.setAttribute("role", "note");
    card.appendChild(srOnly("RichOS status"));
    const body = elem("p", "tl-notice-body");
    body.id = "prose:" + item.id;
    // An app status line is the app's own text and is drawn as written.
    renderMarkdownInto(body, item.text);
    inlineTalkIcon(body);
    card.appendChild(body);
    return card;
  }

  /// **THE OLD CIRCLE GLYPH STANDS FOR THE TALK BUTTON; THE BUTTON IS NOW THE CEO'S VOICE ICON
  /// (2026-10-08), SO A SENTENCE THAT SAYS "tap ◉" SHOWS THAT ICON.** Every ◉ in the text under
  /// `root` becomes a small copy of `#talk-toggle`'s own `<svg>`, sized to the text. The copy
  /// carries the same `talk-icon-*` classes, so it takes the same theme tokens as the button in
  /// light and dark. Without the button in the page (a bare fixture) the glyph is left alone.
  function inlineTalkIcon(root) {
    const src = document.querySelector("#talk-toggle .talk-icon");
    if (!src) return;
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    const hits = [];
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      if (n.nodeValue.indexOf("\u25C9") !== -1) hits.push(n);
    }
    for (const node of hits) {
      const parts = node.nodeValue.split("\u25C9");
      const frag = document.createDocumentFragment();
      parts.forEach((part, i) => {
        if (i > 0) {
          const icon = src.cloneNode(true);
          icon.classList.add("talk-icon-inline");
          // D21: the button's own spoken name, so "tap ___ to talk to me" reads as a sentence.
          icon.removeAttribute("aria-hidden");
          icon.setAttribute("role", "img");
          icon.setAttribute("aria-label", document.getElementById("talk-toggle").getAttribute("aria-label") || "Talk to Rich");
          frag.appendChild(icon);
        }
        if (part) frag.appendChild(document.createTextNode(part));
      });
      node.parentNode.replaceChild(frag, node);
    }
  }

  // -------------------------------------------------------------------------------------
  // TECHY MODE (techy-mode design §3.4) — the SAME rows, with their technical half shown
  // -------------------------------------------------------------------------------------
  //
  // Not a second column and not a side panel. §3.4: *"Inline, in `seq` order, interleaved
  // between the message bubbles of the same turn. A side panel would be tidier and would
  // fail the requirement: the CEO is replacing a terminal, and a terminal is one
  // interleaved stream."* So the interleaving is the one the CEO view already does — the
  // shared per-turn counter (§1.4 G1) — and nothing here re-orders anything.
  //
  // WHAT ARRIVES, AND WHAT DOES NOT. A technical row is an ordinary `activity` item that
  // carries a `detail` object (`{title, summary?, locations, vendorKind?}`), which
  // `Timeline::view(ViewMode::Ceo)` REMOVES and `ViewMode::Technical` keeps. Two kinds of
  // row appear here and nowhere else: a `permission_requested` row (auto-approved by the
  // client and recorded as a fact — NOT a decision awaiting anybody) and an untyped
  // vendor kind, which renders as one dim line carrying its own kind name (§1.4 G5).
  //
  // ===== TWO ROWS THIS DELIBERATELY DOES NOT DRAW, AND THEY ARE NOT OVERSIGHTS ==========
  //
  //   ● thinking ⌄   — §5's day-one mockup opens with it. `agent_thought_chunk` fired ZERO
  //                    times on `claude-agent-acp` 0.70.0, and the native wire is no better:
  //                    7 `thinking` blocks across the 2026-08-31 captures, every one with
  //                    EMPTY text and a signature only. Both paths, in a probe run built
  //                    for nothing else (`MAX_THINKING_TOKENS=10000`, no tools: 17 message
  //                    chunks, 0 thought chunks). Recent models default `thinking.display`
  //                    to "omitted". There is no thought data to render.
  //   fs/read_text_file / fs/write_text_file — never fire either, with both capabilities
  //                    declared and both tools exercised. `ClientFsCall` is real and inert.
  //
  // An affordance that is always empty is a lie about the system: it tells the CEO the
  // model is not thinking and that Rich touched no files, when what is true is that this
  // adapter does not say. Both ROUTES stay built in richos-core so there is no hole the day
  // that changes; neither gets a chevron here today.
  // =====================================================================================

  /// What a technical row is LABELLED with: the merged tool-call title, which after the
  /// §1.4 G2 merge is the real command (`cat engine/VERSION`), never the opening event's
  /// placeholder (`Terminal`, `Preparing file…`).
  ///
  /// An untyped vendor kind has no command, so it is labelled with the vendor's own kind
  /// name — which is what makes §1.4 G5's "one dim line" truthful rather than a shrug.
  function technicalLabel(item) {
    const d = item.detail || {};
    if (d.vendorKind) return d.vendorKind;
    return d.title || item.summary || "";
  }

  /// §5.3 — one row per meaningful action cluster. Subdued, small, semantic.
  ///
  /// The row is a BUTTON because §5.3 says clicking it opens detail. In the CEO view there
  /// IS no detail to open — `Timeline::view(Ceo)` removed it — so the row states what it
  /// knows (its state) and says plainly that the particulars live in technical mode, rather
  /// than opening an empty pane.
  function renderActivityGroup(group, opts) {
    if (group.technical) return renderTechnicalRow(group, opts);
    const row = elem("div", "tl-activity");
    row.dataset.state = group.state;

    const mark = elem("span", "tl-activity-mark");
    mark.setAttribute("aria-hidden", "true");
    mark.textContent = ACTIVITY_GLYPH[group.state] || "·";
    row.appendChild(mark);

    row.appendChild(elem("span", "tl-activity-text", group.label));

    // §18: status must never rely on color alone, and an `unknown` state must not read as
    // done. Only the two states that are NOT self-evident from the verb are spelled out.
    if (["unknown", "failed", "running", "queued"].indexOf(group.state) >= 0) {
      row.appendChild(elem("span", "tl-activity-state", ACTIVITY_STATE_LABEL[group.state]));
    } else {
      row.appendChild(srOnly(ACTIVITY_STATE_LABEL[group.state] || ""));
    }
    return row;
  }

  /// ONE tool call, with its technical half (§3.4: *"One collapsed line per tool call:
  /// status dot, title, path. Expand for input/output."*).
  ///
  /// The head is a `<button>` only when there is somewhere to expand TO. `opts.machineryRaw`
  /// is the seam to §2.4's raw pane and it is supplied by `main.js`; a harness that renders
  /// the same items without it gets the row and no chevron, rather than a control that does
  /// nothing. Same rule the worker chip already follows for `opts.openWorker`.
  ///
  /// STATUS IS CARRIED BY GLYPH **AND** WORD, never by color alone (§18) — and every state
  /// is spelled out here, not just the four the CEO view spells out. In technical mode
  /// `done` is information the CEO is specifically looking at, and `outcome not recorded`
  /// is the honest word for the measured majority case (34 of 58 tool events on 2026-08-28
  /// carried no `status` at all). It is NEVER folded into `done`: "they all finished" is a
  /// completion claim nobody made.
  function renderTechnicalRow(group, opts) {
    const item = group.members[0];
    const detail = item.detail || {};
    const wrap = elem("div", "tl-tech");
    wrap.dataset.state = group.state;
    if (detail.vendorKind) wrap.dataset.vendor = detail.vendorKind;

    const expandable = typeof opts.machineryRaw === "function";
    const expanded = expandable && typeof opts.isMachineryExpanded === "function" && opts.isMachineryExpanded(item.id);

    const head = elem(expandable ? "button" : "div", "tl-tech-head");
    if (expandable) {
      head.type = "button";
      head.id = "mach:" + item.id;
      head.setAttribute("aria-expanded", String(expanded));
      head.addEventListener("click", () => opts.toggleMachinery(item.id));
    }

    const mark = elem("span", "tl-activity-mark", ACTIVITY_GLYPH[group.state] || "\u00b7");
    mark.setAttribute("aria-hidden", "true");
    head.appendChild(mark);

    // The command, in a monospace lane, and NEVER truncated by the renderer. §2.4 already
    // bounded what is stored (an 84-char summary, a 32 KB payload); bounding it a second
    // time here would hide the middle of a command the CEO is reading precisely because he
    // wants to see all of it. Wrapping is style.css's job.
    head.appendChild(elem("span", "tl-tech-title", technicalLabel(item)));
    head.appendChild(elem("span", "tl-activity-state", ACTIVITY_STATE_LABEL[group.state] || "outcome not recorded"));
    if (expandable) {
      const chev = elem("span", "tl-tech-chevron", expanded ? "\u2303" : "\u2304");
      chev.setAttribute("aria-hidden", "true");
      head.appendChild(chev);
    }
    wrap.appendChild(head);

    // The bounded preview and the touched paths sit UNDER the head at all times, not behind
    // the chevron: they are §2.4's normalized record, they are never evicted, and they are
    // the two things that still render after the raw window has passed over this row.
    if (detail.summary) wrap.appendChild(elem("div", "tl-tech-summary", detail.summary));
    if (detail.locations && detail.locations.length) {
      const paths = elem("div", "tl-tech-paths");
      for (const p of detail.locations) paths.appendChild(elem("span", "tl-tech-path", p));
      wrap.appendChild(paths);
    }

    if (expanded) {
      const pane = elem("div", "tl-tech-raw");
      pane.id = "raw:" + item.id;
      // A live region: the raw bytes arrive from a command AFTER this node is mounted, and
      // the three answers it can give (retained / no longer retained / could not be read)
      // are all sentences the CEO has to be told, not states he should have to notice.
      pane.setAttribute("role", "status");
      pane.textContent = "Reading\u2026";
      wrap.appendChild(pane);
      opts.machineryRaw(item.id, pane);
    }
    return wrap;
  }

  /// §7.1 — the delegated-worker group: one summary line, then one chip per worker.
  ///
  /// The chip is a BUTTON (§18: *"worker chips: buttons with name, role and state in
  /// accessible label"*) and it opens the read-only inspector. READ-ONLY IS THE BOUNDARY,
  /// NOT A SIMPLIFICATION: every control §7.2 forbids — interrupt, retry, approve, resume,
  /// model selection, prompt editing — is a business action, and R2 business-action
  /// governance is deferred to V2 by CEO decision for v1 and all 1.x. A window, not a
  /// cockpit.
  ///
  /// Status is carried by GLYPH + WORD, never by color alone (§18). The pulse is on the
  /// `running` chip only, one at a time, and `prefers-reduced-motion` replaces it with a
  /// static mark (§17.4).
  function renderWorkerGroup(group, opts) {
    const wrap = elem("div", "tl-workers");

    const head = elem("div", "tl-workers-head");
    const mark = elem("span", "tl-activity-mark", "→");
    mark.setAttribute("aria-hidden", "true");
    head.appendChild(mark);
    head.appendChild(elem("span", "tl-activity-text", group.label));
    wrap.appendChild(head);

    const chips = elem("div", "tl-chips");
    chips.setAttribute("role", "group");
    chips.setAttribute("aria-label", group.label);
    for (const w of group.workers) {
      chips.appendChild(renderWorkerChip(w, opts));
    }
    wrap.appendChild(chips);
    return wrap;
  }

  function renderWorkerChip(w, opts) {
    const spec = workerStateSpec(w.state);
    const name = workerDisplayName(w);
    const interactive = typeof opts.openWorker === "function";
    const chip = elem(interactive ? "button" : "div", "tl-chip");
    if (interactive) chip.type = "button";
    else chip.setAttribute("role", "group");
    chip.dataset.state = spec.tone;
    chip.dataset.agentId = w.agentId;
    chip.id = "chip:" + w.agentId;

    const glyph = elem("span", "tl-chip-mark", spec.glyph);
    glyph.setAttribute("aria-hidden", "true");
    chip.appendChild(glyph);

    chip.appendChild(elem("span", "tl-chip-name", name));
    // §7.1: "short role or delegated objective". The role is the agent type the harness
    // recorded. The DELEGATED OBJECTIVE is not here and is not faked — see the inspector.
    if (w.agentType && w.agentType !== name) {
      const sep = elem("span", "tl-chip-sep", "·");
      sep.setAttribute("aria-hidden", "true");
      chip.appendChild(sep);
      chip.appendChild(elem("span", "tl-chip-role", w.agentType));
    }

    const state = elem("span", "tl-chip-state", spec.label);
    chip.appendChild(state);
    if (spec.qualifier) chip.appendChild(elem("span", "tl-chip-qualifier", spec.qualifier));
    if (spec.pulse) {
      const pulse = elem("span", "tl-pulse tl-chip-pulse");
      pulse.setAttribute("aria-hidden", "true");
      chip.appendChild(pulse);
    }

    // §18's accessible label: name, role and state — plus the two honest caveats, so a
    // screen-reader user is never told less than a sighted one.
    const parts = [name];
    if (w.agentType && w.agentType !== name) parts.push(w.agentType);
    parts.push(spec.label);
    if (spec.qualifier) parts.push(spec.qualifier);
    if (!workerHasName(w)) parts.push("name not recorded");
    if (w.latestUpdate) parts.push("latest update: " + w.latestUpdate);
    if (interactive) parts.push("open worker details");
    chip.setAttribute("aria-label", parts.join(", "));
    chip.title = spec.note;
    if (interactive) chip.addEventListener("click", () => opts.openWorker(w));
    return chip;
  }

  const ACTIVITY_GLYPH = {
    queued: "·",
    running: "◐",
    completed: "✓",
    failed: "△",
    unknown: "?",
    stopped: "▪",
  };

  // -------------------------------------------------------------------------------------
  // §7.2 THE WORKER INSPECTOR — read-only, and read-only is the BOUNDARY
  // -------------------------------------------------------------------------------------
  //
  // §7.2 lists seven things a worker detail view shows. THREE of them have a source and
  // four do not, and this pane shows the three, names the four, and invents nothing:
  //
  //   1. identity and role                 REAL — `workerName` from a `created` row,
  //                                        `agentType` from whichever row carried one.
  //   2. current state                     REAL — the last state witnessed.
  //      ...and elapsed time               NO SOURCE. §22 lists "elapsed active time" under
  //                                        must-not-be-faked, and the wall-clock spread
  //                                        between two log lines is not active time — a
  //                                        worker that idled two hours between its first and
  //                                        last row did not work for two hours. The two
  //                                        TIMESTAMPS are shown as timestamps; no duration
  //                                        is computed from them anywhere in this file.
  //   3. delegated task in Rich's words    NO SOURCE. The `Task` tool call's title in the
  //                                        measured traffic is the literal string "Task"
  //                                        (timeline_tests.rs:701), and a CEO view carries
  //                                        no `detail` at all — `Timeline::view(Ceo)` removed
  //                                        the bytes rather than hiding them. Surfacing the
  //                                        delegation prompt needs a CEO-safe objective field
  //                                        the projection does not have; see the handoff.
  //   4. latest authored update            REAL — the `summary` a worker actually wrote.
  //   5. output or response when done      NO SOURCE. `result_ref` has no witness.
  //   6. artifacts or changed files        NO SOURCE. Phase 5 owns artifacts.
  //   7. failure reason when failed        NO SOURCE. Nothing in the worker path carries an
  //                                        outcome, so failure is not detectable at all.
  //
  // AND NO CONTROLS. §7.2's forbidden list — model selection, prompt editing, tool
  // permissions, per-worker resume/stop, raw mailboxes — is a list of BUSINESS ACTIONS, and
  // R2 business-action governance is deferred to V2 by CEO decision for v1 and all 1.x. So
  // there is no interrupt, no retry, no approve, and no "ask Sage to..." field. A window,
  // not a cockpit. `renderWorkerInspector` returns a node containing exactly zero
  // interactive elements other than the one disclosure below; the pane's Close button lives
  // in the shell, not here.

  /// An RFC-3339 stamp the emitter wrote, rendered as a TIME. Never differenced.
  function formatStamp(iso) {
    if (!iso) return null;
    const t = Date.parse(iso);
    if (!isFinite(t)) return iso;
    return new Date(t).toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
      second: "2-digit",
    });
  }

  /// The §7.4 sentence for a run that ended with nothing recorded about how. It is Rich's
  /// voice and it is deliberately not a status line: the CEO's real question is "did a
  /// worker fail?", and the only honest answer is that this build cannot tell him — said
  /// once, calmly, without a stack trace and without an alarm.
  //
  // "was cut short" and not "stopped": §6.1 already spends the word `stopped` on a turn the
  // CEO stopped himself (`You stopped after {duration}`), so reusing it here would read as
  // an attribution rather than a possibility.
  const ENDED_EXPLANATION =
    "This run has ended. Nothing recorded whether the work finished, was cut short or failed, so I'm not going to call it either way.";

  function renderWorkerInspector(w, opts) {
    opts = opts || {};
    const frag = document.createDocumentFragment();
    const spec = workerStateSpec(w.state);

    // ---- identity and role (§7.2 item 1) ----
    //
    // The NAME lives in the pane header, which §7.2 requires it to ("The pane header
    // includes a back action, the worker name and a close action"), so it is not repeated
    // here — printing it twice, twenty pixels apart, made the pane read like two panes.
    // It is still in this fragment's accessible structure via the role line's heading when
    // the header is absent (the isolated-render case in `tests/workers.js`).
    const idBlock = elem("div", "insp-identity");
    idBlock.appendChild(elem("h3", "insp-name sr-only", workerDisplayName(w)));
    if (w.agentType) idBlock.appendChild(elem("p", "insp-role", w.agentType));
    if (!workerHasName(w)) {
      idBlock.appendChild(
        elem("p", "insp-note", "This run was first seen already underway, so no display name was recorded for it.")
      );
    }
    frag.appendChild(idBlock);

    // ---- state (§7.2 item 2, minus the elapsed time that has no source) ----
    const stateBlock = elem("div", "insp-block insp-state");
    stateBlock.dataset.state = spec.tone;
    const line = elem("p", "insp-state-line");
    const glyph = elem("span", "insp-state-mark", spec.glyph);
    glyph.setAttribute("aria-hidden", "true");
    line.appendChild(glyph);
    line.appendChild(elem("span", "insp-state-word", spec.label));
    if (spec.qualifier) line.appendChild(elem("span", "insp-state-qualifier", spec.qualifier));
    stateBlock.appendChild(line);
    stateBlock.appendChild(elem("p", "insp-state-note", w.state === "unknown" ? ENDED_EXPLANATION : spec.note));
    frag.appendChild(stateBlock);

    // ---- the latest thing this worker actually wrote (§7.2 item 4) ----
    // THE WORKER RESULT, in the only form that exists. §7.2 says the result stays visible
    // when the activity closes, and that is why this sits ABOVE the disclosure below rather
    // than inside it.
    if (w.latestUpdate) {
      const upd = elem("div", "insp-block insp-update");
      upd.appendChild(elem("p", "insp-label", "Latest update"));
      upd.appendChild(elem("p", "insp-update-text", w.latestUpdate));
      frag.appendChild(upd);
    }

    // ---- the observed chronology, collapsible INDEPENDENTLY (§7.2) ----
    const chron = elem("div", "insp-block insp-chron");
    const open = !!opts.chronologyOpen;
    const toggle = elem("button", "insp-disclosure");
    toggle.type = "button";
    toggle.id = "insp-chron-toggle";
    toggle.setAttribute("aria-expanded", open ? "true" : "false");
    toggle.setAttribute("aria-controls", "insp-chron-body");
    const chev = elem("span", "tl-chevron", open ? "⌄" : "›");
    chev.setAttribute("aria-hidden", "true");
    toggle.appendChild(chev);
    toggle.appendChild(elem("span", null, "What I saw"));
    if (typeof opts.toggleChronology === "function") toggle.addEventListener("click", opts.toggleChronology);
    chron.appendChild(toggle);

    const body = elem("dl", "insp-facts");
    body.id = "insp-chron-body";
    if (!open) body.hidden = true;
    const fact = (k, v) => {
      if (v == null) return;
      body.appendChild(elem("dt", null, k));
      body.appendChild(elem("dd", null, v));
    };
    fact("First seen", formatStamp(w.firstObservedAt));
    fact("Last seen", formatStamp(w.lastObservedAt));
    fact(
      "Lifecycle events",
      typeof w.eventsObserved === "number" ? String(w.eventsObserved) : null
    );
    // The two timestamps above are LABELS. §22 forbids faking elapsed active time and the
    // spread between them is not it, so the pane says so where the number would have gone.
    body.appendChild(elem("dt", null, "Time spent working"));
    body.appendChild(elem("dd", "insp-absent", "not recorded"));
    chron.appendChild(body);
    frag.appendChild(chron);

    // ---- what is genuinely not here (§7.2 items 3, 5, 6, 7) ----
    // Said once, in one sentence, rather than four empty sections. A pane that silently
    // omitted them would read as broken; a pane that listed four "unavailable" rows would
    // read as an apology. This states the boundary and moves on.
    // Shortened to what is still true once the output record exists (output side-panel PRD
    // §12.10): the files a worker touched are listed, under its name, in the Output panel.
    const gap = elem("p", "insp-note insp-gap");
    gap.textContent = "I don't have this worker's brief or its output. The files it touched are in Output.";
    frag.appendChild(gap);

    return frag;
  }

  /// The disclosure's label when its turn has work to show and no number. `updateTimers`
  /// keeps it on every tick, so a live answered turn does not lose its name a second later.
  const DISCLOSURE_WITHOUT_NUMBER = "What Rich did";

  /// §6 — the working-duration row, and §6.4's disclosure.
  ///
  /// The row IS the disclosure control (§6.4 step 5). What it discloses is the ACTIVITY
  /// lane only — never prose. See the header of this file.
  function renderDurationRow(turn, opts) {
    const row = durationRow(turn.record, opts.now);
    // "Work" now includes delegated workers (§6.4 lists worker lifecycle rows inside the
    // collapsed transcript). A turn whose ONLY work was a delegation therefore gets a real
    // disclosure control instead of a static row — before this it got the static one and
    // the delegation had nowhere to be.
    const hasActivity = turn.stream.some(isWork);
    // No number and nothing to disclose: no row at all, rather than one reading "Worked".
    if (!row.label && !hasActivity) return { node: null, row, hasActivity };
    // No number, but work to disclose: the control names what it opens.
    if (!row.label) row.label = DISCLOSURE_WITHOUT_NUMBER;
    const wrap = elem("div", "tl-duration");
    wrap.dataset.tone = row.tone;

    let control;
    if (hasActivity) {
      control = elem("button", "tl-duration-btn");
      control.type = "button";
      control.id = "duration:" + turn.turnId;
      const open = opts.isExpanded(turn.turnId);
      control.setAttribute("aria-expanded", open ? "true" : "false");
      control.setAttribute("aria-controls", "work:" + turn.turnId);
      const chev = elem("span", "tl-chevron", open ? "⌄" : "›");
      chev.setAttribute("aria-hidden", "true");
      control.appendChild(chev);
      control.addEventListener("click", () => opts.toggle(turn.turnId));
    } else {
      control = elem("div", "tl-duration-btn tl-duration-btn--static");
    }

    const label = elem("span", "tl-duration-label", row.label);
    label.dataset.turnId = turn.turnId;
    control.appendChild(label);

    if (row.live) {
      const pulse = elem("span", "tl-pulse");
      pulse.setAttribute("aria-hidden", "true");
      control.appendChild(pulse);
    }

    // §6.4: "The timer's live updates must not be announced to screen readers every
    // second." The log region is aria-live="off" and this name is read only on focus.
    let name = row.label;
    if (row.note) name += ". " + row.note;
    if (hasActivity) {
      name += opts.isExpanded(turn.turnId) ? " Hide what Rich did." : " Show what Rich did.";
    }
    control.setAttribute("aria-label", name);
    control.title = row.note || "";

    wrap.appendChild(control);
    // *Wrote N files* (output side-panel PRD §6.5): the turn's own line says what it wrote, from
    // the output record, and opens the panel on this turn's group. A SIBLING of the disclosure,
    // never inside it — a button in a button is not a control anybody can operate.
    const wrote = turnFiles(opts, turn.turnId);
    if (wrote.length) {
      const sep = elem("span", "tl-wrote-sep", "·");
      sep.setAttribute("aria-hidden", "true");
      wrap.appendChild(sep);
      const b = elem("button", "tl-wrote", wrote.length === 1 ? "Wrote 1 file" : "Wrote " + wrote.length + " files");
      b.type = "button";
      b.id = "wrote:" + turn.turnId;
      b.addEventListener("click", () => {
        if (outputLinks && outputLinks.openGroup) outputLinks.openGroup(turn.turnId, b);
      });
      wrap.appendChild(b);
    }
    wrap.appendChild(elem("span", "tl-rule"));
    return { node: wrap, row, hasActivity };
  }

  /// **The files Rich put in front of the user, under his words** (round 36 screen 3): inside
  /// his last message of the turn, right after its prose and above its Copy row, as the round
  /// draws them. A turn with no words of his (a file Stu attached for a job) gets them in the
  /// lane on their own.
  function placeFiles(lane, turn, opts) {
    const nodes = [];
    if (turn.files && turn.files.length && window.RichFiles) nodes.push(window.RichFiles.render(turn.files, opts));
    // The "Your instructions · Saved" card sits in the same place (round 36 screen 1).
    if (turn.instructions && window.RichYourInstructions) {
      for (const item of turn.instructions) nodes.push(window.RichYourInstructions.card(item));
    }
    for (const node of nodes) placeUnderWords(lane, node);
  }

  function placeUnderWords(lane, list) {
    let last = null;
    for (const child of lane.children) if (child.classList.contains("tl-rich")) last = child;
    if (!last) {
      lane.appendChild(list);
      return;
    }
    let actions = null;
    for (const child of last.children) if (child.classList.contains("tl-rich-actions")) actions = child;
    last.insertBefore(list, actions);
  }

  /// The files the output record lists under this turn, newest first, or none.
  function turnFiles(opts, turnId) {
    return opts.output && opts.output.forTurn ? opts.output.forTurn(turnId) : [];
  }

  /// *Produced 3 files* and their names as links, under Rich's last words in the turn (§6.5).
  function renderProduced(entries) {
    const strip = elem("p", "tl-produced");
    strip.appendChild(
      elem("span", "tl-produced-label", entries.length === 1 ? "Produced" : "Produced " + entries.length + " files")
    );
    for (const e of entries) strip.appendChild(fileLink(e, e.name));
    return strip;
  }

  /// **THE ONE LABEL FOR THE ONE ACTION** — the nightly's D6.
  ///
  /// It read "Pick it back up", and the audit measured what pressing it did: his text went
  /// back in the composer. No new turn, no log line, nothing picked back up. The words
  /// promised that Rich would resume and he does not — deliberately, because resending
  /// spends his subscription and starts work, and that is his to trigger.
  ///
  /// So the label is what the control actually does, and the phrasing is lifted from the
  /// one sentence in this file that was already accurate about it — the unknown card's
  /// *"Your message is safe: I'll put it back in the box for you."* Declared once, used by
  /// both cards, because the CEO's job is identical in each.
  const RETRY_LABEL = "Restore my message";

  /// §5.5 — a system intervention. Exactly ONE is reachable in this build.
  ///
  /// §5.5 lists six: waiting for a CEO answer, action approval, permission, connection
  /// lost, recovery after restart, unrecoverable failure. Five have no source (live.rs
  /// deferrals; `waiting_for_user` does not exist; approvals are auto-approved and recorded
  /// as a completed FACT, not a decision awaiting anyone; recovery is `Internal` and never
  /// announced). The sixth — a turn that ended without finishing — is reachable, and its
  /// CEO-facing signal is the duration row's own state, so this card is driven by that and
  /// not by a `SystemError` item (which is `Technical` and correctly never arrives here).
  ///
  /// THE RAW REASON IS STILL NOT SHOWN. `cognition io: broken pipe` is implementation
  /// machinery; §21 asks for "a short Rich-voiced explanation", and the two sentences below
  /// are what the shipping build says for an ordinary `rich://turn-error`.
  ///
  /// ## THE UPSTREAM CASE, AND WHY IT IS NOT THE SAME CARD (open-items row 3.30)
  ///
  /// When the turn died because the model API failed, `turn.outage` carries an
  /// `upstream_outage` item and its three sentences REPLACE the two generic ones. That is
  /// not a widening of "show the reason" — every sentence on that item was AUTHORED in
  /// `crates/richos-core/src/upstream.rs`'s `ceo_message` functions, written to the ledger
  /// at the moment of the failure, and is relayed here verbatim. This renderer composes
  /// nothing, edits nothing and interpolates nothing; it is the backend-authored relay
  /// `tests/lib/state-strings.js` documents as blind spot B2 and scrapes for the affordance
  /// registry.
  ///
  /// **The generic note is REPLACED rather than kept, and that is the point.** It reads
  /// *"Everything I'd already written above is saved"* — true about the text, and a claim
  /// about the whole that is false: the session's working context is gone, and on
  /// 2026-09-03 five agents died having written nothing at all. The outage item's
  /// `lossMessage` is built from counts read off the ledger, so it says what is on disk AND
  /// what is not.
  ///
  /// **The button stays, and its label does not change.** Asking again is still the one
  /// thing the CEO can do, whichever way the turn died; a second verb for one action is a
  /// second thing to learn.
  /// ## THE LOCAL CASE, AND WHY IT IS THE SAME SHAPE AS THE UPSTREAM ONE (nightly D2)
  ///
  /// `turn.interruption` carries a `turn_interruption` item whose sentences REPLACE the two
  /// generic ones, exactly as the outage item's do, and for exactly the same reason: every
  /// one of them was authored in `crates/richos-core/src/interruption.rs` and written to the
  /// ledger at the moment of the failure. This renderer composes nothing.
  ///
  /// **And it is the one place the RETRY CONTROL can be absent.** The published nightly of
  /// 2026-09-17 failed a turn because nobody was signed in, said "I hit a snag mid-thought",
  /// promised that work was saved when nothing had been written, and offered "Pick it back
  /// up" — a control that could not succeed however many times it was pressed. So the button
  /// is drawn from `offersRetry`, a boolean the backend decided, never from the prose. A
  /// control that cannot work is worse than no control: pressing it and watching nothing
  /// happen is how somebody stops believing what the app tells them.
  ///
  /// **The route is a sentence, not a link.** `Settings → Account connection` names a screen
  /// that exists; it is rendered as a note because RichOS is voice-first and a route that
  /// only works as a click is a route that fails when it is read aloud.
  function renderFailureCard(turn, opts) {
    const card = elem("aside", "tl-intervention");
    card.setAttribute("role", "note");
    const interruption = turn.interruption;
    if (interruption && interruption.ceoMessage) {
      // What happened. Authored by `InterruptionCause::ceo_message`.
      card.appendChild(elem("p", "tl-intervention-body", interruption.ceoMessage));
      // What is on disk and what is not. Authored by `InterruptionRecord::new`, and ABSENT
      // when nothing of Rich's had been written — which is the sentence D2 got wrong. No
      // element at all, rather than an empty one.
      if (interruption.lossMessage) {
        card.appendChild(elem("p", "tl-intervention-note", interruption.lossMessage));
      }
      // HIS WORDS COME BACK, AND THAT IS NOT THE SAME QUESTION AS `offersRetry`.
      //
      // The candidate-.2 walk, §4 defect #8: the not-signed-in card tells him to *"send this
      // to me again"*, the composer is empty, and — unlike the `[Errno 2]` card beside it —
      // the card offered no **Restore my message**. The one thing it asked for was the
      // one thing it did not give him. He retypes, or he loses the sentence.
      //
      // `offersRetry` is `InterruptionCause::offers_retry`, and it answers *"is asking again
      // a plan?"* — `false` for a missing sign-in, a rejected credential and a missing
      // provider, correctly, because no number of presses clears any of those. D2 built this
      // branch on it when the control was called **Pick it back up**, which really did
      // promise a resume. D6 then renamed the control to what it actually does — it puts the
      // text back in the composer and starts nothing — and NOBODY REVISITED THE GATE. That is
      // the whole of this defect: a boolean that was right for the old label, left in front
      // of the new one.
      //
      // So the gate is now the question the control's own words ask. There are words to put
      // back, or there are not. It is `renderUnknownCard`'s rule verbatim, one screen down in
      // this same file — *"NO BUTTON WHEN THERE IS NOTHING TO PUT BACK … a control that puts
      // nothing back is worse than no control"* — and D2's rule is untouched by it: a control
      // that CANNOT WORK is still worse than no control, and this one always works. It does
      // not resend, does not spend his subscription, does not start a turn, and does not
      // claim the failure has cleared. The sentence above it still says asking again will not
      // help, where that is true.
      //
      // `stopped-by-ceo` is the one cause excluded, and by its TAG rather than by reading its
      // prose: `interruption.rs` says of it *"his words go back in the box by the ordinary
      // route rather than through a failure card's control"*, so the words are already there
      // and a second control would put them back twice.
      const wordsToPutBack = !!(turn.user && turn.user.text) && interruption.cause !== "stopped-by-ceo";
      if (wordsToPutBack) {
        card.appendChild(retryControl(turn, opts));
      }
      return card;
    }
    // The same rule for the cards below: Restore my message only when there is a message of
    // his to restore. A capacity or usage-limit failure on a turn a button or an agent started
    // keeps its explanation and gets no control (review rv-20261019T115204Z-2b619840-5405#1).
    const hisText = !!(turn.user && turn.user.text);
    const outage = turn.outage;
    if (outage && outage.ceoMessage) {
      // What happened. Authored by `UpstreamFault::ceo_message`.
      card.appendChild(elem("p", "tl-intervention-body", outage.ceoMessage));
      // What is on disk and what is not. Authored by `TurnLoss::ceo_message`.
      if (outage.lossMessage) card.appendChild(elem("p", "tl-intervention-note", outage.lossMessage));
      // What was spent trying. Authored by `RetryBudget::ceo_message`. ABSENT on the first
      // failure, which is a different statement from "nothing was spent" — so the line is
      // absent too rather than reading "0 attempts".
      if (outage.retryMessage) card.appendChild(elem("p", "tl-intervention-note", outage.retryMessage));
    } else {
      card.appendChild(elem("p", "tl-intervention-body",
        "Rich's answer was cut off before he finished."));
      if (hisText) {
        card.appendChild(elem("p", "tl-intervention-note",
          "Press Restore my message to put your words back in the box."));
      }
    }
    if (hisText) card.appendChild(retryControl(turn, opts));
    return card;
  }

  /// **THE CONTROL SAYS WHAT IT DOES** — the nightly's D6.
  ///
  /// `docs/verification/2026-09-17-nightly-1.2.0-20260917.1-onscreen-audit.md` §D6:
  /// *"The message says 'say the word and I'll pick it back up'; the button says 'Pick it
  /// back up'. Pressing it PUTS HIS TEXT BACK IN THE COMPOSER — no new turn, no log line.
  /// Returning the text is useful behavior; the words promise that Rich will resume, and he
  /// does not."*
  ///
  /// **The behavior is right and it is the words that changed.** Resending has side effects
  /// — it spends his subscription and starts work — so it is his to trigger, which is the
  /// rule `renderUnknownCard` below already states. A control that silently resent would be
  /// a worse product, not a more honest one.
  ///
  /// So the label is now what actually happens, and it borrows the metaphor from the one
  /// sentence in this file that was already accurate about it: *"Your message is safe — I'll
  /// put it back in the box for you."* One verb across both cards, because the CEO's job is
  /// identical in each and two phrasings for one action is two things to learn.
  ///
  /// The generic body above lost *"say the word and I'll pick it back up"* in the same pass,
  /// for the same reason: it was the sentence that made the promise the button could not
  /// keep.
  function retryControl(turn, opts) {
    const retry = elem("button", "tl-intervention-action", RETRY_LABEL);
    retry.type = "button";
    retry.id = "retry:" + turn.turnId;
    retry.addEventListener("click", () => opts.retry(turn));
    return retry;
  }

  /// §14's other card: a turn that was still in flight when the app last closed, whose
  /// outcome nothing recorded.
  ///
  /// UNTIL THIS COMMIT IT SAID "Send it again if you still need it." AND CARRIED NO BUTTON.
  /// That sentence is an instruction to act, addressed to a reader whose message is no
  /// longer in the composer — the app had cleared it on send and the turn it belonged to
  /// never came back. So the one thing the card asked for was the one thing it did not
  /// offer. A state the user could change, rendered apart from the control that changes it,
  /// is not a status; it is a request.
  ///
  /// It borrows the FAILURE card's control verbatim — same class, same verb, same handler —
  /// because the CEO's job here is identical (get my words back so I can decide) and two
  /// phrasings for one action is two things to learn. `opts.retry` puts the text back in
  /// the composer and focuses it; it does NOT resend. Resending is an action with side
  /// effects and it is his to take (main.js `retryTurn`).
  ///
  /// NO BUTTON WHEN THERE IS NOTHING TO PUT BACK. A turn first witnessed mid-flight has no
  /// `turn.user.text`, and a control that puts nothing back is worse than no control — so
  /// the note says what is true instead and claims nothing.
  function renderUnknownCard(turn, opts) {
    const card = elem("aside", "tl-intervention tl-intervention--quiet");
    card.setAttribute("role", "note");
    card.appendChild(elem("p", "tl-intervention-body",
      "This turn was still running the last time RichOS was open and I can't tell you how it ended."));
    const canRetry = !!(turn.user && turn.user.text);
    card.appendChild(elem("p", "tl-intervention-note",
      canRetry
        ? "Anything I'd written is above. Your message is safe: I'll put it back in the box for you."
        : "Anything I'd written is above. Nothing of yours was lost."));
    if (canRetry) {
      const retry = elem("button", "tl-intervention-action", RETRY_LABEL);
      retry.type = "button";
      retry.id = "resume:" + turn.turnId;
      retry.addEventListener("click", () => opts.retry(turn));
      card.appendChild(retry);
    }
    return card;
  }

  /// One turn: CEO bubble, then the duration row, then the lane.
  ///
  /// LAYOUT DECISION, AND ITS LIMIT. §5.4 wants the final response "separated from work
  /// activity by the completed duration row", with the response below it. This build cannot
  /// identify the final response, so the duration row sits above ALL of Rich's prose: every
  /// run is below the divider, the deliverable among them. That satisfies §25's "the final
  /// response appears below the completed-duration divider" for the ordinary case and never
  /// mislabels a run as the answer.
  function renderTurn(model, turn, opts) {
    const frag = document.createDocumentFragment();
    const section = elem("section", "tl-turn");
    section.dataset.turnId = turn.turnId;

    if (turn.user) section.appendChild(renderUserMessage(turn.user, opts));

    // A turn with NO record is one of exactly two things, both real and both short-lived:
    // an optimistic CEO bubble in the instant before `turn-status` names a turn id, and a
    // locally-authored NOTICE — a voice-mode failure, a send that never reached the desk.
    // Both get this lane and NO duration row (there is no turn to have taken any time), and
    // the notice is drawn as a status line rather than as speech (finding #8).
    if (!turn.record) {
      const lane = elem("div", "tl-lane");
      for (const item of turn.stream) {
        if (item.kind === "question") lane.appendChild(window.RichQuestions.render(item, opts));
        else if (isLocalNotice(item) && item.text) lane.appendChild(renderLocalNotice(item));
        else if (isProse(item) && item.text) lane.appendChild(renderRichMessage(item, opts));
      }
      placeFiles(lane, turn, opts);
      if (lane.childNodes.length) section.appendChild(lane);
      frag.appendChild(section);
      return frag;
    }

    {
      const { node, row, hasActivity } = renderDurationRow(turn, opts);
      if (node) section.appendChild(node);

      const expanded = !hasActivity || opts.isExpanded(turn.turnId);
      const lane = elem("div", "tl-lane");
      lane.id = "work:" + turn.turnId;

      // Interleaved in shared-counter order. When the transcript is COLLAPSED the activity
      // rows are omitted and the prose rows stay exactly where they were — so expanding
      // restores the original chronology in place (§6.4, §25).
      let pendingActivity = [];
      const flush = () => {
        if (!pendingActivity.length) return;
        if (expanded) {
          for (const g of rollupWork(pendingActivity)) {
            lane.appendChild(
              g.type === "worker" ? renderWorkerGroup(g.group, opts) : renderActivityGroup(g.group, opts)
            );
          }
        }
        pendingActivity = [];
      };
      for (const item of turn.stream) {
        if (isWork(item)) {
          pendingActivity.push(item);
          continue;
        }
        flush();
        if (item.kind === "question") lane.appendChild(window.RichQuestions.render(item, opts));
        else if (isLocalNotice(item) && item.text) lane.appendChild(renderLocalNotice(item));
        else if (isProse(item) && item.text) lane.appendChild(renderRichMessage(item, opts));
      }
      flush();

      // §6.4: "The collapsed row shows a chevron and optionally one summary line."
      if (hasActivity && !expanded) {
        const groups = rollupWork(turn.stream.filter(isWork));
        // The SEMANTIC summary even in technical mode. §6.4 allows "optionally one summary
        // line", and a collapsed turn joined from ten full shell commands is not a summary
        // line — it is the expanded view with the newlines taken out.
        const summary = groups
          .map((g) => (g.group.technical ? g.group.members[0].summary : g.group.label))
          .join(" · ");
        const line = elem("button", "tl-collapsed-summary", summary);
        line.type = "button";
        line.id = "summary:" + turn.turnId;
        line.setAttribute("aria-label", "Show what Rich did: " + summary);
        line.addEventListener("click", () => opts.toggle(turn.turnId));
        // Above the prose, in the position the activity itself occupies when expanded.
        lane.insertBefore(line, lane.firstChild);
      }

      // The files Rich attached sit right under his words (round 36 screen 3), above the
      // record's own *Produced* strip.
      placeFiles(lane, turn, opts);

      const produced = turnFiles(opts, turn.turnId);
      if (produced.length) lane.appendChild(renderProduced(produced));

      section.appendChild(lane);

      if (row.tone === "ceo-stopped" && turn.user && turn.user.text) {
        section.appendChild(retryControl(turn, opts));
      }
      // A turn that carried no words of his (Rich answering Stu's report, an app message, a
      // button) has no message to restore (nightly 67 p3: "Press Restore my message ... Your
      // message is saved" for a message he never sent). Its card keeps only a cause he can
      // act on — `offersRetry` false: sign in, replace the key, reinstall Claude Code (review
      // rv-20261019T113734Z-68e37b15-cc60#1, a "Start the questions" press whose sign-in had
      // expired) — and is not drawn for a cause whose only advice is restoring his message.
      // The duration row above still says it stopped. `hisWords` absent (an older build's
      // payload) keeps the whole card.
      const notHis = !!(turn.interruption && turn.interruption.hisWords === false);
      if (row.tone === "stopped" && !notHis) section.appendChild(renderFailureCard(turn, opts));
      if (row.tone === "stopped" && notHis && !turn.interruption.offersRetry
          && turn.interruption.cause !== "stopped-by-ceo" && turn.interruption.ceoMessage) {
        const card = elem("aside", "tl-intervention");
        card.setAttribute("role", "note");
        card.appendChild(elem("p", "tl-intervention-body", turn.interruption.ceoMessage));
        section.appendChild(card);
      }
      if (row.tone === "unknown") section.appendChild(renderUnknownCard(turn, opts));
    }

    frag.appendChild(section);
    return frag;
  }

  // -------------------------------------------------------------------------------------
  // TURN REUSE — why `render` is no longer `innerHTML = ""`
  // -------------------------------------------------------------------------------------
  //
  // MEASURED, NOT ASSUMED. `render` is called on every STRUCTURAL change: every activity
  // row, every worker upsert, every turn-status transition. It used to empty the container
  // and rebuild every turn. On the 10,000-item thread three design documents promise
  // ("A 10,000-item thread history remains smooth"), in the WebKit Tauri ships on, that
  // cost **256 ms per new row** — a fifteen-frame stall, once per tool call, for the whole
  // of a long turn. The numbers, all six shapes, are in
  // `docs/verification/timeline-scale-2026-08-30/baseline.txt` and `scale.js` pins them.
  //
  // So a turn whose rendered inputs are unchanged keeps the nodes it already has. The
  // signature below is what "unchanged" means, and it is deliberately built from OBJECT
  // IDENTITY rather than a field list: every write to an item goes through `putItem` (or,
  // since this commit, the delta path), both of which REPLACE the object, so a new field on
  // any item kind invalidates its turn without anyone remembering to add it here. Equivalent
  // same-view snapshots retain the old item object; actual changes still replace it. A field
  // list is the thing that rots.
  //
  // THE ONE CONTRACT THIS PLACES ON CALLERS: the `opts` callbacks (`copy`, `retry`,
  // `toggle`, `openWorker`, `rerender`) must be behaviorally stable between renders,
  // because a reused node keeps the listeners bound when it was built. `main.js` passes
  // module-level functions and the harness passes equivalent closures. Everything a
  // callback closes over that CAN change — the item, the group, the expanded set, the
  // `expandedMessages` membership of a clamped bubble — is in the signature.
  //
  // NOT DONE HERE, AND NAMED RATHER THAN IMPLIED: this does not window the DOM. Every turn
  // is still MOUNTED. What keeps that affordable is `content-visibility: auto` on `.tl-turn`
  // (style.css), which is the engine's own virtualization — offscreen turns are skipped for
  // layout and paint. Both halves were needed: the CSS alone left 136 ms of teardown and
  // style recalc per row, and the reuse alone left a 51,000-node layout on every scroll.
  const renderCache = new WeakMap(); // container -> { model, sections: Map<turnId, entry> }
  const itemTokens = new WeakMap(); // item object -> stable identity token
  let itemTokenSeq = 0;

  function tokenOf(obj) {
    if (!obj) return "0";
    let t = itemTokens.get(obj);
    if (t === undefined) {
      t = ++itemTokenSeq;
      itemTokens.set(obj, t);
    }
    return t;
  }

  /// Everything `renderTurn` reads, in one string. `avatarIn` is part of it because the
  /// Rich Hand mark is a once-per-SESSION decision that runs THROUGH the turns in order: a
  /// turn that would draw the avatar renders differently from the same turn after some
  /// earlier turn has drawn it.
  function turnSignature(model, turn, opts, avatarIn) {
    const parts = [
      avatarIn ? "a1" : "a0",
      opts.isExpanded && opts.isExpanded(turn.turnId) ? "e1" : "e0",
      opts.openWorker ? "w1" : "w0",
      // The turn RECORD is mutated in place (`t.status`, `t.live`, `t.activeMs`), so it is
      // the one thing here that cannot be signed by identity.
      turn.record ? JSON.stringify(turn.record) : "r0",
    ];
    if (turn.user) {
      // `turn.user` is a fresh projection object every call; the ITEM behind it is not.
      parts.push(
        "u" +
          tokenOf(model.items.get(turn.user.id)) +
          (turn.user.steering ? "s" : "") +
          (turn.user.pending ? "p" : "") +
          (opts.expandedMessages && opts.expandedMessages.has(turn.user.id) ? "x" : "")
      );
    } else {
      parts.push("u0");
    }
    // The outage row is drawn INSIDE the failure card and is not in `turn.stream`, so it
    // needs its own term here. Without it, a turn whose failure card is already on screen
    // would keep the cached node when the outage item arrives, and the authored sentences
    // would never appear — the same class of bug the raw-pane term below exists for.
    parts.push(turn.outage ? "o" + tokenOf(model.items.get(turn.outage.id)) : "o0");
    // The interruption row is drawn inside the same card and is likewise absent from
    // `turn.stream`, so it needs its own term for the same reason — and it decides whether
    // the retry button is drawn at all, so a stale cached node here is a control that
    // should have disappeared and did not.
    parts.push(
      turn.interruption ? "i" + tokenOf(model.items.get(turn.interruption.id)) : "i0"
    );
    for (const item of turn.stream) {
      // The open/closed state of a raw pane is NOT a property of the item object, so it
      // cannot ride in on `tokenOf`. Without this the CEO clicks a chevron, the turn's
      // signature is unchanged, the cached node is reused and nothing happens — the exact
      // class of bug the identity-based signature exists to avoid everywhere else.
      const open = opts.isMachineryExpanded && opts.isMachineryExpanded(item.id) ? "+" : "";
      parts.push(tokenOf(item) + open);
    }
    // The files Rich attached are drawn from their own slot, not the stream, so each needs its
    // own term: a file attached after the turn is on screen must replace the cached node.
    for (const item of turn.files || []) parts.push("rf" + tokenOf(item));
    // So does a "Your instructions · Saved" card that arrives after the turn is on screen.
    for (const item of turn.instructions || []) parts.push("yi" + tokenOf(item));
    parts.push(opts.machineryRaw ? "m1" : "m0");
    // The Oxford comma setting changes how Claude's words are drawn without any item changing.
    parts.push(window.RichOxford && window.RichOxford.keeping() ? "k1" : "k0");
    // The output record changes what a turn draws — its links, *Wrote N files*, the produced
    // strip — without any item changing, so its revision is part of every turn's signature.
    parts.push(opts.output && opts.output.rev ? "f" + opts.output.rev() : "f0");
    return parts.join("|");
  }

  /// Full render into `container`. Called on every STRUCTURAL change; streaming text and
  /// the one-second timer tick both bypass it (see `updateProse` / `updateTimers`), so a
  /// turn that streams for two hours rebuilds the DOM once per new item, not once per
  /// token (§15's "coalesce tiny deltas to avoid layout thrash").
  function render(model, container, opts) {
    const turns = turnsOf(model);
    outputLinks = opts.output || null;

    let cache = renderCache.get(container);
    if (!cache || cache.model !== model) {
      // A different model is a different thread (or a different fixture). Nothing carries
      // over — including whatever `renderFirstRun` or a previous caller put in here.
      cache = { model: model, sections: new Map() };
      renderCache.set(container, cache);
      container.textContent = "";
    }
    const previous = cache.sections;
    const sections = new Map();
    const nodes = [];

    let avatarShown = opts.avatarAlreadyShown === true;
    const renderOpts = Object.assign({}, opts, {
      showAvatar: () => {
        if (avatarShown) return false;
        avatarShown = true;
        return true;
      },
    });
    for (const turn of turns) {
      const avatarIn = avatarShown;
      const sig = turnSignature(model, turn, opts, avatarIn);
      const hit = previous.get(turn.turnId);
      if (hit && hit.sig === sig && hit.node.parentNode === container) {
        avatarShown = hit.avatarOut;
        sections.set(turn.turnId, hit);
        nodes.push(hit.node);
        continue;
      }
      // One Rich identity per TURN. The avatar is still once per SESSION (the Rich Hand mark
      // is a greeting, not a speaker label), so the two counters are separate on purpose.
      let identityShown = false;
      const turnOpts = Object.assign({}, renderOpts, {
        showIdentity: () => {
          if (identityShown) return false;
          identityShown = true;
          return true;
        },
      });
      const node = renderTurn(model, turn, turnOpts).firstElementChild;
      sections.set(turn.turnId, { node: node, sig: sig, avatarOut: avatarShown });
      nodes.push(node);
    }
    cache.sections = sections;

    // Place `nodes` in order, moving what is already right into place and removing whatever
    // is left over. Everything before `cursor` is already correct, so a node that needs to
    // move can only be AFTER it — one `insertBefore` per mismatch, no second pass.
    let cursor = container.firstChild;
    for (const node of nodes) {
      if (cursor === node) {
        cursor = cursor.nextSibling;
        continue;
      }
      container.insertBefore(node, cursor);
    }
    while (cursor) {
      const next = cursor.nextSibling;
      container.removeChild(cursor);
      cursor = next;
    }
    return turns;
  }

  /// The streaming path: one node, one rebuild of THAT node's children, no turn rebuild and
  /// no focus move.
  ///
  /// IT RUNS THE SAME MARKDOWN AS THE STRUCTURAL PATH, deliberately, rather than holding
  /// plain text until the message closes. Half a subset is worse than either whole one: the
  /// answer would stream as literal asterisks and then snap into bold at the end, which
  /// reads as a glitch on exactly the surface this fix exists to make look right.
  ///
  /// The cost was MEASURED rather than assumed, under WebKit on this machine, over an answer
  /// of the shape Rich actually sends (a heading, bold, italic, code spans, a bulleted list,
  /// a numbered list, six paragraphs). WebKit clamps `performance.now()` to 1ms, so a single
  /// build is unmeasurable and the figures below are a mean over a 500-build batch:
  ///
  ///     786 bytes  ->  0.0240 ms per parse-and-build
  ///    3938 bytes  ->  0.1300 ms per parse-and-build
  ///
  /// `scheduleProse` (main.js) coalesces on `requestAnimationFrame`, so this runs at most
  /// once per frame per streaming message. At 3938 bytes that is 0.1300 / 16.7 = 0.78% of a
  /// 60Hz frame — and a four-kilobyte answer is a long one.
  function updateProse(container, messageId, text, closed) {
    const node = container.querySelector('[id="prose:' + cssEscape(messageId) + '"]');
    if (!node) return false;
    renderMarkdownInto(node, said(text), outputLinks);
    node.classList.toggle("is-streaming", !closed && !!text);
    return true;
  }

  /// The §6.2 tick: "The active label updates once per second. Do not emit timer events
  /// every second. Persist timestamps and derive the display locally." One write per live
  /// row, and NOT inside a live region — §6.4 forbids announcing the timer every second.
  function updateTimers(model, container, nowMs) {
    let anyLive = false;
    for (const [turnId, t] of model.turns) {
      // §58: a turn whose question is still being answered ticks too, even though the turn
      // itself completed seconds after he sent it. Without this the label would be drawn once
      // and then freeze at "Checking for 1s".
      if (!t.live && !questionInFlight(t)) continue;
      anyLive = true;
      const node = container.querySelector('[data-turn-id="' + cssEscape(turnId) + '"].tl-duration-label');
      if (!node) continue;
      const row = durationRow(t, nowMs);
      const btn = node.closest(".tl-duration-btn");
      // The same fallback `renderDurationRow` gives a disclosure with no number to show
      // (handover review of 742df82c1): without it, the tick wrote the empty label over it.
      const label = row.label || (btn && btn.getAttribute("aria-expanded") !== null ? DISCLOSURE_WITHOUT_NUMBER : "");
      if (node.textContent !== label) node.textContent = label;
      if (btn) {
        let name = label;
        if (row.note) name += ". " + row.note;
        const control = btn.getAttribute("aria-expanded");
        if (control !== null) name += control === "true" ? " Hide what Rich did." : " Show what Rich did.";
        btn.setAttribute("aria-label", name);
      }
    }
    return anyLive;
  }

  function cssEscape(s) {
    return String(s).replace(/(["\\])/g, "\\$1");
  }

  // -------------------------------------------------------------------------------------
  window.RichTimeline = {
    PENDING_TURN,
    DURATION_MEANING,
    CHECK_BECOMES_INVESTIGATE_MS,
    setQuestionsInFlight,
    setScreenWaits,
    hasLiveRow,
    createModel,
    bind,
    accepts,
    applySnapshot,
    addPendingUserMessage,
    dropPendingUserMessage,
    markSendRejected,
    markStopping,
    addLocalNotice,
    inlineTalkIcon,
    onTurnStatus,
    onCeoMessage,
    onMessageStarted,
    onMessageDelta,
    onMessageCompleted,
    isMachineryExpanded,
    toggleMachinery,
    onActivityUpserted,
    onWorkerUpserted,
    isTurnExpanded,
    toggleTurn,
    turnsOf,
    formatDuration,
    durationRow,
    rollupActivity,
    rollupWork,
    rollupWorkers,
    workerGroupSummary,
    workerStateSpec,
    workerDisplayName,
    teammateName,
    joinNames,
    renderWorkerInspector,
    ENDED_EXPLANATION,
    render,
    renderMarkdownInto,
    updateProse,
    updateTimers,
  };
})();
