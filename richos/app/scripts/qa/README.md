# `scripts/qa/` — the walk toolkit

The helpers a QA walk needs, committed once, so that no walk writes them again.

## The rule

**A QA brief names the tool from this directory. A walker who needs a helper that
is not here ADDS it here and commits it — never a throwaway under the scratch
directory.**

That is the whole point. Across the eight walks of 2026-09-18 to 2026-09-20,
**111 helper scripts** were written from scratch: 6, 3, 20, 7, 22, 21, 20 and 12
per run. They were the same nine or ten jobs every time, and every rewrite was a
chance to get a measurement subtly different from the last run's.

The contrast calculation alone was written **ten times across five walks**, under
five different names, with **five different rules** for picking the text color out
of a region. They do not agree: on antialiased type the rules differ by more than
a full ratio point. So walk-to-walk contrast numbers have not been comparable,
against a threshold — 4.5:1 — whose entire value is that it is the same number
every time.

## The tools

Every one takes `--help`, exits non-zero with a sentence when it cannot answer,
and never prints a number it did not measure.

| Tool | The job |
|---|---|
| `episode-counter.py` | Read-only Claude/Codex episode accounting: request usage, tool-time unions, non-tool gaps and unresolved verification candidates. Emits numeric aggregates and source locators only. |
| `caption-wer.py` | A transcript against a video's captions (yt-dlp's WebVTT, rolling YouTube cues read once): word error rate on normalized words, and the capitalization and spelling differences counted by pair, plus the most repeated phrase on each side (a decoder loop shows there first). Added with the video-skill walk of 2026-10-07. |
| `contrast.py` | WCAG ratio of two colors, or of a region of a frame. One estimator, stated in the file, printed with every answer. |
| `frame.py` | Read the frame: `px`, `crop`, `extent`, `inset`, `box`, `edges`, `motion`. `inset` is the four-sided gap measurement behind the 18/18 settings-button check. |
| `ocr-gate.sh` | The privacy gate: no frame enters the record carrying an address, a home path or a listed person's name. Refuses to report clean until a positive control proves the reader works. `--text <file-or-dir>` also scans a walk record's text files (step lists, the record) for an operator home path (the guest's own `/Users/admin` is allowed), an address or a listed name, with its own positive control. |
| `redact.py` | Cover an address (found by OCR, tokens joined across a line), a phone-shaped digit run (`--phones`, the same shape `ocr-gate.sh` flags), a `/Users/<name>` home path (`--homes`, the gate's home-path shape; added with the candidate 38 walk) or an exact rectangle, then **re-read the output** and fail if anything survived. |
| `ocr-find.sh` | Which of these frames shows this text? Exit 0 on a hit, **1 on none**. |
| `ocr-watch.sh` | Sample a region on an interval and read it out with timestamps — a countdown, a status line, a pane being paged through. |
| `timeline.py` | `capture` an action and its frames **on one clock**; `--also-region` adds a SECOND rectangle captured around the same press, into `<outdir>/b/`, carrying the same action instant — the Mac and the phone page answered off one clock. `report` the first repaint against the frame before the action; `at` dates a frame something else chose (the one `ocr-find.sh --first` named) off that same clock; `stats` for min/median/max over samples. |
| `wait-for.sh` | Wait for a git ref to move, a log line, a file to appear or be gone (`--gone`: a workspace another agent holds), a wall-clock instant, or a new process. **Exit 1 on timeout** — a wait that gives up is a failure. |
| `phone-client.mjs` | A headless phone: the **production** mobile client (`richos/mobile/core/client.js`) paired to a real install's phone listener. `pair` prints the six words and waits for the walker's `match`/`mismatch` in a decision file; `send` times acknowledgement, first and completed reply on one clock; `state`, `forget`; `foreign` presents this phone's credential to a DIFFERENT Mac and **exits 1 if that Mac accepts it**. No simulator, no physical phone. |
| `phone-android.py` | **Run it as `randroid device --serial S <command>`** (CEO 2026-10-02: only the release build goes on a physical phone and every test on it tests that build; started any other way it refuses, and it refuses a phone holding a debuggable RichConnect). A **physical** Android phone, driven the way a person drives it and read the way the closure matrix needs: `tap`/`wait`/`gone`/`node`/`texts` by the words on screen, `swipe` a Recents card, `type-file` one `input text` per character, `keys` through the on-screen keyboard's own keys (a key map), `field` to read the editable text back, `unlabeled` for clickable controls a screen reader would announce without a name, `shot` and a timed `burst` of frames, `idle-frames` (frames and their rhythm while untouched), and a read-only `state` of what an app leaves running (process and birth, CPU ticks, services, wake locks, live microphone, jobs, alarms, sockets, its own notifications), with `compare` for two of them and `observe` for one whole closure-matrix cell (action, settle, two readings, comparison). Refuses an unnamed or unattached serial: `native-android/bin/emu-ui.py` is the emulator-only counterpart. |
| `lab-ledger.py` | Did each message reach the **isolated** test Mac exactly once, word for word, with one reply? Reads the lab's `timeline.json`, exits 1 on a missing, duplicated or altered message, and refuses a timeline without the isolated lab's owner marker. Prints counts, never the conversation. |
| **`lab-phone`** (`../../../mobile/native-ios/Tools/LabPhone`) | The iPhone app's own Swift core (store, network handler, `URLSession`) headless on this Mac, paired to an **isolated** lab through the managed route, repeating the send-then-Home cycle (`--mode fresh` relaunches before each send) with the core's `send:` account of every step. With `lab-pause.py --log` on its `--marks` file, a send is kept in flight across "Home" as on the phone: about two minutes, no phone, no UI-automation approval, no background allowance spent. `--pose wifi-drop` adds the iPhone walk's network drop while hidden (`--offline-at`, `--offline-for`, marking "RELEASE MAC n" when it starts), `--away-before S` the runner's trip away from the app between tries; every run logs each stream's open, answer and first bytes, and prints how long after each return the reply was in the core's state (`replyAfterReturnSeconds`). A lab without a managed route is reached through its own loopback proxy, the production `URLSession` transport unchanged. Exit 1 when any message stayed until the return. It cannot show iOS itself (UIKit's background time, suspension, the radio): a send that leaves here but stays on the phone points at the phone. Build with `swift build --package-path … --scratch-path /Volumes/E1TB/…`; refuses a lab without the owner marker. |
| `lab-pause.py` | Hold the **isolated** lab Mac silent between two marks of a phone's step log: SIGSTOP at the first `mark` line (`--stop-at`), SIGCONT at the second (`--cont-at`, optional `--cont-delay`), `{n}` for try n of `--times`. One process only, the lab's own listener from its `mac.json`; refuses a lab without the owner marker or a pid that is not the listener, and always continues it (each try, a timeout, its own stop). `--log-dir` waits for the next run's test log. For the Mac-goes-quiet cell and for a send kept in flight across Home. |
| `step-mark.py` | Append one line, the words then this Mac's epoch seconds, to a step log: `phone-android.py` keeps none, and `lab-pause.py` follows a text file line by line. One process, one append. |
| `hidden-send-try.py` | **Run it as `randroid device --serial S hidden-send ...`** (refused otherwise, and on a debuggable build). One try of a send kept in flight across Home on a PHYSICAL Android phone, with the marks `lab-pause.py` follows: `PAUSE MAC n`, tap Send, Home, optionally Wi-Fi off under the request, `RELEASE MAC n`, Wi-Fi on (always put back). `--metrics-port` samples the lab tunnel helper's request counters and `--sockets-uid` the app's own TCP sockets, so a second request is told from a late answer. A frozen lab alone only makes the request succeed late: the app's read timeout is 30 s, longer than the 5 s bound after Home. |
| `tunnel-requests.py` | How many requests reached the isolated lab through its tunnel, and when: `sample` reads the lab's Connect helper's request counters (`--port`, its `--metrics` port) every 0.25 s on this Mac's clock, one line per reading; `between` counts the rise between two instants, each rise bracketed by its readings. The iPhone half of telling a second request with the same client id from a late answer (the phone has no shell). Refuses a port without the counter, a window the readings miss and an unreadable reading inside it. |
| `flake-rate.sh` | How often does a command fail? `--runs N LABEL 'COMMAND' [LABEL 'COMMAND' …]` runs every label once per round, interleaved, so a before/after pair sees the same host load; counts pass, fail and (with `--admit`) not-admitted; keeps and names only the failing logs. `--nice` is CEO §78's low-priority run on a busy Mac. |
| `under-load.py` | Run ONE command on a Mac carrying a known CPU load: `--workers N` one-core workers (default cores + 2, a load average above the core count, as with three nightly gates side by side), a `reserve.py` host sample before and after, the command's own exit status. Stops only the workers it started, and each worker also leaves when its stdin pipe closes, so a killed run leaves nothing spinning. Put it behind `../testvm/reserve.py --` so a busy Mac refuses it first; keep the command short. |
| `stall-run.py` | Run ONE command whose process group is repeatedly held stopped (`--run-ms R --stop-ms S`, default 1 ms running then 2,500 ms stopped) while its children in their own groups, and the wall clock, carry on: a test process descheduled on a busy Mac, made certain rather than likely. A check that decides by wall time fails under it; one whose clocks are only hang guards passes. The proof tool of the R9 fixes (`docs/verification/2026-09-29-load-sensitive-checks-audit.md`). Signals only the group it started, never after its own child has exited, and continues the group however it ends short of SIGKILL. |
| `slow-shutdown.py` | Run ONE program that starts shutting down a known time after it is signaled: SIGTERM, SIGINT or SIGHUP is forwarded to its only child `SLOW_SHUTDOWN_MS` (or `--ms`) later, and it exits with the child's status. Pass it where a harness takes a program path (`CHROME_PATH=…/slow-shutdown.py SLOW_SHUTDOWN_TARGET=<real Chrome> SLOW_SHUTDOWN_MS=1000` for the extension browser suites): a program descheduled on a busy Mac, made certain. Cleanup that waits for the program to exit passes under any value; cleanup that sleeps a fixed time and then removes the folder the program still writes to fails. Signals only the child it started, never after that child has exited. |
| `busy-sample.py` | Run ONE command with every `../testvm/reserve.py` admission sample in it reading a given load (`--busy PERCENT`, default 100; memory pressure normal, no swap-out), adding no load at all. A check whose verdict follows the host sample changes with `--busy`; one that does not passes under any value. The companion of `stall-run.py` (which takes the CPU from the test process) and `under-load.py` (which loads the Mac, without making the reading land where a check decides). Works on every Python process in the command's tree started without `-S`/`-I`. |
| `phone-ios.py` | A physical iPhone through its real controls. **`run`, `approval`, `procs`, `apps`, `lock`, `battery` and `syslog` run as `rios device <command> ...`** (refused otherwise; the walk runs only the Release build). `check` also states a list's expected time at the phone's measured median step times and the `--allowance` it needs, and `run` refuses a list its allowance cannot hold: XCUITest stops at the allowance and every later step, shot and tree is lost. iOS 26 has no shell on the phone, so a list of steps (launch, tap, type, Home, lock, shot, accessibility audit…) runs as one XCUITest check (`rios device verify script`) and every step comes back as one line on the phone's clock, with its shots. The list is validated here before anything is built. `appearance` reads and sets the phone's own light/dark (the app follows the phone; set it back), `orientation` turns the phone and reports the app window's size. `tapThen` taps its target and, a set 0-2 s later, touches a screen point, as one synthesized event record (a touch at an exact moment of an app's return); every `tap` logs `tapAt`, the phone-clock moment it was asked for. `run --screen-recording` keeps XCTest's own recording of the phone's screen for the session (made on the phone) and names it in the summary. A step may act `in` iOS Settings (the app's permissions, Wi-Fi; set back what you change) or Safari (the one share-sheet source, reached with `open` on an https link); `tree` and `shot` keep that app's window, `launch` starts it fresh on its first page, and `kind` with `label` is one switch by its label. `summary DIR` reads a finished run back: each step's outcome, time and audit issues, and each shot or tree by its name. `vocabulary` exits 1 when a step this tool validates has no case in the phone runner (`PhysicalDeviceTests.perform`), before a build finds it. `syslog-rate` counts one process's entries per time bucket of a `syslog` capture (what the app did while hidden). `syslog --keep TEXT` (6+ characters, repeatable) also keeps the system lines containing TEXT, never the whole log: `--keep IOHIDEventSystem` shows backboardd's HID clients, and its `SetProperty called IOHIDEventSystemClientIsUnresponsive ... pid:N process:NAME` lines name the owner of each unresponsive client (the phone's condition). Committed, unchanged app source is never rebuilt or re-signed: the one signed build of that tree is reused from the shared store, so a walk starts in seconds, and no tool ever uninstalls the app or its runner. Before anything is built, `run` forecasts whether the phone will ask its owner to allow UI automation (`approval` prints the same) and refuses an expected-to-ask run until `--approval-announced` says the CEO was told. Also read-only `procs`, `apps`, `lock` and `battery`. |
| **`blank.py`** (`../../../mobile/perf/blank.py`) | Does the phone show a blank screen, or content that jumps, after the user taps the app (CEO, 2026-10-02)? Reads a screen recording of either phone (decoded by macOS AVFoundation through `framedump.swift` beside it; nothing installed), measures from the tap (`--tap-at`, or the still home screen's first visible reaction), and lists every blank stretch (white, one flat color, a spinner on an empty screen; a logo is not blank; the opening animation of an empty window counts) and every jump, each with its start and length. `analyze` judges one start against `BLANK_LIMIT_MS` (400 ms, the one place that number lives), `series` judges several by their median (p95 from 20) and lists every start over the limit, `frames --png DIR` shows what each frame was judged. Exit 1 on FAIL, 2 when it cannot answer. Phone recordings stay private on the external SSD. Its cold-start series runs inside `perf.py android` (`blankstart.py`); the static launch-screen check is `launchscreen.py` beside it. |
| `pair-words.py` | The six pairing words under **pairing v2**, both sides: `mac` derives the isolated lab Mac's words from its own files (origin, CA, the key the phone registered; refused without the lab's owner marker), `phone` reads the words the phone showed from its PHONE_STEP log, `check` **exits 1 when any word differs**. The lab's `ready.json` still prints the v1 words, so going by it gives a false mismatch. Use with `phone-ios.py pair-steps --v2-hold S`: check during the hold, stop your run on anything but exit 0. |
| `usage-shape.sh` | What shape is Claude Code's usage answer, and could RichOS read it? Asks a `claude` (`--claude`, `--home`/`--config-dir` for its login) exactly as the quota reader does (`quota/probe.rs`: same flags, `initialize` then `get_usage`) and prints only keys, booleans and whether each window's utilization is a number, never a figure, token or session field. **Exit 1 when the reader gets no figures from the answer**, naming the rule: `NO READING` for `rate_limits is null` (the reader's `NoReading`, no figures this time, since `2bb7359ad`), `UNREADABLE` for a shape it refuses; 2 when there is no binary or no answer. Run it in the guest with a steps-walk `push` and `guest` step (added with the candidate 36 walk, where the panel said "could not read" for a real account). |
| `fixtures/make-fixtures.py` | Regenerate the committed fixtures, or `--check` that they still match. |
| **guest screen and shell:** `../testvm/ax.sh`, `../testvm/guest.sh` | Not in this directory, and looked for here first every time. `ax.sh <vm> tree\|find\|click` is the guest's accessibility tree with real geometry and a press by title or description; `guest.sh <vm> <command>` is one word into the guest, with `--pull`/`--push`. They live beside the VM they need (`scripts/testvm/`, documented in `docs/testvm.md`); twenty-five throwaways were written before they existed. `../testvm/steps-walk.py <vm> --steps FILE.json --out DIR` runs a JSON list of those operations (`ax`, `shot`, `tree`, `guest`, `push`, `handfile`, `until`, `wait`, and `relaunch`, which is `../testvm/relaunch.py` on the walk's own guest) as one walk inside `run-walk.py`, recording every step; a walk's clicks are data, not a new script (added with the candidate 33 walk; `relaunch` with S8, so "survives a relaunch" is a step too). |

`lib/qaimg.py` and `lib/qaocr.py` are the shared halves: one PNG reader (Pillow
when it imports, a built-in decoder when it does not — the candidate .11 walk
hand-rolled one in a heredoc because Pillow was missing), one luminance, one
ratio, one tesseract lookup.

## Two things that are not negotiable

**Nothing here photographs the operator's screen.** `timeline.py capture` and
`ocr-watch.sh` refuse unless they are running as the test VM's guest user or the
caller has set `RICHOS_QA_CAPTURE=allow` deliberately. His Mac is never a test
surface; the app under test lives in the VM (`scripts/testvm/`).

**The list of people is not in this repository and never will be.** `ocr-gate.sh`
owns the SHAPE patterns — address-shaped, `/Users/<name>`-shaped, phone-shaped —
and hands the OCR text to `engine/scripts/lib/named-persons.py` for the roster,
which lives at `~/.richos-privacy/named-persons`, outside every work tree. Five
walks each inlined their own half-remembered copy of that roster into a scratch
script; committing one here would publish it, which is worse than any of them.
`ABSENT` — no list on this machine — makes the gate exit 2, because "nothing was
checked" is not "nothing to check".

## Running the tests

    bash scripts/qa.test.sh

The suite lives at `scripts/qa.test.sh`, one level up, **on purpose**:
`run-tests.sh` builds its inventory with `find -maxdepth 1`, so a suite inside
this directory would run in no build and appear in no `--only` list — exactly how
`scripts/testvm/test/run-tests.sh` became invisible, as `proof-for.sh`'s own
comment records. 53 cases, about twelve seconds, no VM, no build, no window.

Roughly half the cases drive a tool into its refusal, because that is the failure
each one was built against: a wait that timed out and returned success, a gate
whose reader was blind, a redactor that never re-read its output, an estimator
that took a stray antialiasing pixel for the ink.

## Adding a tool

One job per file. `--help` that says what it does. A non-zero exit with a
sentence when it cannot answer. A case in `scripts/qa.test.sh` that proves both
the answer and the refusal. Add the row to the table above, and add the path to
this suite's `# run-tests: inputs` line if it lands outside `scripts/qa`.

## Read-only execution episode accounting

`episode-counter.py` accepts explicit local transcript roots and inclusive/exclusive
UTC bounds. Redirect its JSON output to a private report, never a public fixture:

```sh
python3 -B scripts/qa/episode-counter.py --claude-root "$CLAUDE_CONFIG_DIR/projects" \
  --codex-root "$CODEX_HOME/sessions" --since 2026-09-20 --until 2026-09-27 \
  > "$PRIVATE_REPORT"
```

The tool reads a fixed byte extent of each selected file, deduplicates streamed
Claude usage by response ID and supports Codex response-ID usage records. Older
Codex usage without response IDs remains unknown. Input-token semantics differ:
Claude uncached input and Codex total input must not be added as equivalent values.
A background tool result measures handoff latency, not its process lifetime.
Non-tool gaps include scheduling and user pauses, not just inference.

UI candidates require three consecutive tool calls that execute screen/OCR tools.
An intervening image-read call ends that strict sequence. Verification candidates
require two recognized runner calls in a task with no more than 30 minutes between
calls; source equality is unknown until full source receipts are reviewed.
Observation candidates require three consecutive polling calls with the same
command/arguments and byte-identical text output. Different output is not assumed
unchanged. Shell matching is conservative and lexical, not a complete shell parser;
Codex JavaScript wrappers require literal double-quoted command arguments.

Per-episode timing includes intervening tools in the selected span. Different
families can overlap; never sum them as disjoint total session cost. Canary
mentions and claims that only the canary failed are separate unresolved evidence,
not passing verification and not established agent waste. The reference interval
option `--reference FILE FIRST LAST` reproduces a manually selected episode.
No state is written, no commands are executed and no transcript text is printed.
Default per-file pacing targets light CPU use; run at low scheduling priority on
an already busy host. This counter does not bypass test resource admission.

Episode accounting schema 2 treats `agent_hold.py wait` as collection rather
than a new logical operation. It links explicit native task receipts, preserves
missing completion as unknown and reports foreground tool time separately from
the upper bound between background launch and collection. Help invocations are
excluded. Scripted walk invocations are candidates in `scripted_ui`; their inner
calls can be counted from explicitly supplied `--walk-evidence steps.json` or
`screen.json`. These records contain no model usage and are never added to model
request totals. The miner does not evaluate commands or read paths from transcript
arguments. Verification candidates still require qualified source evidence before
being called unchanged-source retries or avoidable waste.
