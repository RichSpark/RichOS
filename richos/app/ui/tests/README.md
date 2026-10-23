# `richos/app/ui` browser acceptance harness

A home for the browser tests every UI slice was otherwise re-inventing and throwing away.
Slices 3, 5 and 7 each wrote one; only this one survived and only because it has a
directory.

```
npm install          # once, in THIS directory — installs Playwright AND downloads WebKit
npm test             # every suite in this directory, discovered from disk
node workers.js      # one suite, while you are working on it
```

That is the whole setup, from a clean checkout, with no path into any other repository. It
was not, until 2026-08-30. **Playwright 1.61 ships no `postinstall` of its own**, so
`npm install` here used to produce the JS API and not one browser engine — and the suites ran
anyway, on this machine, because a webkit binary from an unrelated project was already sitting
in the shared `~/Library/Caches/ms-playwright`. Six consecutive runs of this directory were
launched with `RICHOS_PLAYWRIGHT` pointed into another repository's `node_modules` and the
line above said they did not need to be. `package.json` now carries
`postinstall: playwright install webkit`, which downloads 77 MiB the first time on a machine
and takes under a second on every worktree after that, because the cache is shared.

If you would rather not have that download at all, the escape hatch is still there and is
still supported — it is now an opt-out rather than the only way in:

```
RICHOS_PLAYWRIGHT=/path/to/node_modules/playwright node run.js
```

## Making this machine behave like a runner

The first public `ui-suite-ci` runs, 2026-09-04, were red on nine checks between them and
every one was a check that had folded the harness's own latency or the runner's own speed
into a claim about the product. Two knobs exist so the next one of those can be reproduced
here instead of argued about:

```
RICHOS_SPLASH_LAG_MS=2000 node splash.js
```

`page.goto()` resolves on `load` and the opening curtain goes up long before that. The gap
is 51-72 ms on this machine and was ~2,000 ms on a GitHub `macos-latest` runner, which is
what made six splash checks red at once. This delays the harness's first instruction after
every launch by that many milliseconds, so a fast machine measures what a slow one measures.
Zero and no delay at all, unless it is set.

**`home.js` reads the same variable, deliberately** — `RICHOS_SPLASH_LAG_MS=2000 node home.js`
— because it is the same condition and one condition should not need two switches. It is what
reproduced run 33933067025's `the loading state was already dismissed before the field was
live`: the check sampled the surface once, from outside, at whatever moment the harness got its
turn and on a runner that moment was after the picture had finished. Three failures out of
three with the lag in, on a launch whose order was correct throughout. The check now records
the order in the page and asserts THAT, so the harness's turn cannot reach it.

```
RICHOS_FRAME_BUDGET_MS=30 node scale.js
```

The reverse direction and the same lesson from the other side. `scale.js` asserted 60fps over a
10,000-item thread and was red on every public run — 17ms here, 23ms median and 68ms p95 on a
`macos-latest` runner with three vCPUs and no GPU. That number is about the machine, so it is no
longer a gate anywhere in CI: it is measured and printed every run and this turns it back into a
gate on hardware where the answer means something. What holds the promise in CI instead is a
count that reads the same on any machine (the drag does zero DOM writes and zero model reads) and
a ratio that divides the machine out of both halves (1,000 turns against 120, same motion).

```
RICHOS_SPLASH_BUDGET_MS=16.7 node splash.js
RICHOS_SLIDE_ABS_PX=1        node home.js
```

Two more of the same shape, added 2026-09-05 for the two checks that were still red after the
pair above. Both are OFF by default, both numbers are measured and PRINTED on every run either
way and both name in their own suite what is no longer gated.

`splash.js` check 13 asserted that the curtain costs the launch under one frame — 16.7ms — and
was red on every public run. It had already been given a measured noise floor and that was not
enough: on run 33957510095 the floor came out at 18.5ms and the warm arm's difference at 54.5ms.
The gate is now COUNTS, which read the same on any machine: 0 Tauri commands, 0 fetches, 0 XHRs,
0 subresources, 30 DOM nodes and all 30 of them inside `#splash`, nothing `main.js` can await
and — per frame, over the whole ceremony — at most one layout read and never one after a write.
Check 13b measures the milliseconds, prints both arms beside that run's own noise floor and this
variable turns them back into a gate on hardware with GPU compositing.

`home.js`'s entity-row slide asserted that the company name's VIEWPORT position never moves
backwards. It is the sum of two separately-clocked transitions — the track's `transform`, which
WebKit runs on the compositor and the pill's `width`, which is layout — inside a CENTERED row,
so they subtract; measured here they start one frame apart and the viewport position goes
backwards on 20 runs out of 20, worst step 6.85px. The slide is now read IN THE PILL, where it is
one animation and monotonic at zero tolerance and the two halves are additionally required to be
one declaration (same duration, same delay, same curve). This variable puts the old viewport
assertion back at its old ±1px.

```
RICHOS_SPLASH_SHUTTER_LAG_MS=400 node splash.js
```

Added 2026-09-05 and it is a SECOND lag knob because the first one cannot reach the check that
needed it — which is worth stating plainly, since the obvious move was to reuse it.

`lag()` is applied immediately after `goto`. The next thing `settledShot` does is wait for
`state.barStopped`, which the bar does not report until 3,950 ms into the curtain. So 2,000 ms
of `RICHOS_SPLASH_LAG_MS` lands the harness at ~2,050 ms and is then absorbed WHOLE by the wait
behind it: run it and check 5 is not stressed by one millisecond. That knob is right for checks
8, 12b, 12c and 22, which sample at FIXED INSTANTS; it is structurally incapable of stressing a
check that waits for a state to arrive.

What a slow runner costs check 5 is spent AFTER the bar stops — the round trip, the settle and
the full-viewport screenshot of a composition of gradients and filters, which is the expensive
one. This delays exactly that span, between the bar landing and the shutter opening, which is
the span the window is measured across. At `400` it reproduces the failure on this machine; at
`3000` it demonstrates that the fixed shutter no longer has a deadline at all.

**The window it is about, derived and then measured.** The shutter opens at `holdMs + FLARE_MS`
and the curtain is removed at `holdMs + CEILING_GRACE_MS + FADE_MS + 40`:

```
(holdMs + 1000 + 180 + 40) - (holdMs + 950) = 270 ms
```

`holdMs` cancels, so a screen that asks for five seconds hands the camera the same 270 ms a
three-second one does — measured 271 ms at `seconds: 3` and 265 ms at `seconds: 5` and the
removal instant measured at 4,221 ms against 4,220 derived. That is why check 5 disarms the
ceiling for its two launches (`NO_CEILING`) rather than asking for a longer screen and why
widening `CEILING_GRACE_MS` in the product was refused: it would change what a launch does on
the CEO's machine in order to suit a screenshot. Check 10b asserts the disarm stays an opt-in —
two call sites, read off the suite's own source — and checks 10 and 12c still watch a real
failsafe fire.

**It drives BOTH shutters and that is why there is no third variable.** `matShot` — the mat
photograph checks 15 and 18 are built on — waits for a state too (`atCurtain(page, 2200)`, on
the page's own clock), so `RICHOS_SPLASH_LAG_MS` is absorbed whole by that wait below 2,200 ms
and stops being a lag above it. This is applied at the same seam in both: between the wait that
precedes the picture and the picture.

`matShot`'s guard was the same DOM-presence test check 5 was green over, with margin instead of
none. Measured: its shutter opens at 2,304-2,336 ms and its `seconds: 5` ceiling is armed for
6,000 ms, so the margin is 3,664-3,696 ms — distant, not unreachable, because `matShot` does not
disarm anything. `splash--yielding` lands at 6,001-6,024 ms over four launches and the node
leaves 220-223 ms later, against 220 derived from `FADE_MS + 40`. Inside those 220 ms the node
is present and the curtain is gone, which is exactly what `up` cannot see:

```
RICHOS_SPLASH_SHUTTER_LAG_MS=3720 node splash.js
```

Before the fix that was **green**: check 18 filed `material-round-11-v1.png` with 62,346 distinct
colors against the mat's 2,951 and check 15 reported 99%/100% where it reports 64%/77% — the
"material reaches the mat" comparison run over two photographs of the home screen. After it, both
go red naming the instant and the guard refuses before publishing, so no wrong picture is filed.
At `0` and `3000` both are green and the filed pair is the same picture — 2,951 / 9,922 distinct
colors against 2,951 / 9,923 — so it is invariant across the whole margin and past it the check
refuses rather than files.

The guard is the CLASS, never computed opacity and there is a second reason for that beyond the
one on `curtainNow`: opacity does not order the damage. A shutter reading 0.41 came out with
87,336 distinct colors and one reading 0.32 with 11,039.

The other knob is not a variable: `home.js`'s WebGL check PLANTS the failure it is about
(`getShaderParameter` refusing COMPILE_STATUS), because this machine's WebGL works and a
check that passes for want of the defect is not a check. Do the same for anything else the
runner can do and this machine cannot.

### A loaded nightly host, on demand (2026-09-29)

The nightly runs the UI shards beside the mutation pool, the script suites and Cargo and on
that host a check that measured wall time, raced the page's own timers or slept and then
asserted something had happened went red for a product that was right
(`docs/verification/2026-09-29-load-sensitive-checks-audit.md`, R1, R2, R6, R10, R11). Two
preloads put that host on any Mac, deterministically, without loading anyone's machine:

```
node --require ./fixtures/slow-round-trips-preload.js home.js
RICHOS_UI_ROUND_TRIP_LAG_MS=2000 RICHOS_UI_ROUND_TRIP_LAG_ONLY='THE TEMPORARY LINE' \
  node --require ./fixtures/slow-round-trips-preload.js home.js
RICHOS_UI_ROUND_TRIP_LAG_MS=0 RICHOS_UI_PAGE_BUSY_MS=600 \
  node --require ./fixtures/slow-round-trips-preload.js retention.js
```

`slow-round-trips-preload.js` makes every harness round trip to the page (`evaluate`,
`waitForFunction`, `click`, `goto`, keyboard, mouse) answer late while the page's own timers
keep running (`_ONLY` confines it to checks whose name matches) and `RICHOS_UI_PAGE_BUSY_MS`
holds each page's own main thread busy so its timers and frames come late.

```
NODE_OPTIONS="--require $PWD/fixtures/stalled-host-preload.js" \
  RICHOS_UI_STALLED_HOST_ONLY='^navigation-hang-suite\.js$' RICHOS_UI_SPAWN_LAG_MS=6000 node navigation-evidence.js
```

`stalled-host-preload.js` makes THIS process late: it freezes for
`RICHOS_UI_FREEZE_AFTER_GOTO_MS` right after each navigation is issued and delays every
`spawnSync` by `RICHOS_UI_SPAWN_LAG_MS`.

A check that is green without them and red with them was measuring the host. The fixes all
take one of three shapes and a new check should start from one of them rather than from a
`waitForTimeout`: **read at the page's own instant** (`page.clock` paused and advanced with
`runFor`, as `home.js`'s temporary-line check does or `pinClock` to the fixture's instant, as
`memory-strategy.js` does); **wait for the fact the next line asserts**, with a bound that is
only a hang guard; or, for an absence, **wait for the thing that must be absent to have
ARRIVED** (a delivered event, the page's own frames), never for a number of milliseconds.
`awaitSettled`, `parkPointer` and `leaveHome` now throw, naming what they waited for, instead
of handing a caller a wrong state when their bound runs out.

`node_modules/` and `.shots/` are gitignored; `package-lock.json` is COMMITTED, because
`npm ci` is the only install command that refuses to resolve anything not already written
down and it does not run without one. The tests are the artifact; those PNGs are evidence for
one run.

**`shots-26/` and `shots-5b/` are the exceptions and ARE committed.** §26 names nine screenshots as
deliverables of the memory-strategy fixture and until slice 8 this UI had no visual record
at all — the display on this machine has been locked for three slices and `screencapture`
returns a valid single-color (0,0,0) PNG. `memory-strategy.js` writes those nine, out of
WebKit's own compositor, pixel-verified. Two of the nine are deliberately not what §26 asked
for; the filenames and the suite's SCREENSHOT INVENTORY say which and why. `shots-5b/` is
the same arrangement for the correction desk: twelve states, written by `corrections.js`,
with `shots-5b/README.md` naming what each one is evidence of. `shots-5/` is the same again
for the feedback channel — nine states, written by `feedback.js`, with its own README.

### A COMMITTED SHOT IS WRITTEN ONLY WHEN THE PICTURE CHANGED

This used to say those directories were "overwritten on every run and not byte-stable; read the
suite's exit code, not a `git diff` over a PNG". That was true and it was the wrong thing to
settle for. Measured 2026-09-05 against `5e00651`: one `node run.js` left **96 of the 96
committed PNGs modified**, with byte deltas from -12 to +679,604 — so `git status` was dirty
after every run, `git checkout -- richos/app/ui/tests/` became a habit (four times in one day, each one
a chance to throw away a real change) and a genuine visual regression would have arrived as one
more modified PNG in a list of ninety-six.

`lib/harness.js` now decodes both sides with `lib/png.js` and writes a shot **only when a
decoded sample actually differs**. When one does change, the run says so in a line naming the
pixel count and the worst channel delta, so the diff arrives explained rather than as a binary
blob. Any doubt writes: an unreadable or unfamiliar PNG compares as different, because a
redundant write costs a line in `git status` and a wrongly-skipped one costs a regression nobody
sees.

Three sources of run-to-run churn were removed with it and each is measured in the comment that
fixed it:

* **the opening curtain** — `leaveHome` now clears it and waits for it to be gone, so a suite no
  longer photographs the app either side of a three-second fade (`shots-7-2/` differed by 84% and
  94% of its pixels between two runs);
* **looping animations** — pinned to phase 0 for the length of the capture (the `WORKING` pulse
  dot alone accounted for three files);
* **unfinished transitions** — waited out through `Animation.finished` rather than through a
  `waitForTimeout` somebody guessed (the settings panel photographed at 99-point-something
  percent of its fade) and the splash bar is photographed once it reports it has landed rather
  than at an arbitrary point in its run.

### RE-MEASURED 2026-09-19: THE CHURN WAS MOSTLY STALENESS, AND IT WAS COSTING REAL CHANGES

Seven consecutive same-source runs of every suite that writes shots — five from a clean tree
at `5f3a1a1e` and two more from the regenerated tree at `a1c65c3a` — one directory at a time. 131 committed shots in 13 directories. The result splits in
two and the bigger half was not what anybody was looking at:

* **104 of the 131 differed from the committed copy** — not because anything was unstable, but
  because the product had changed and nobody had regenerated them. The settings button had moved
  about 12px (2,305 pixels of difference, repeated identically across some sixty shots), the
  phone sheet had grown a footer bar of three controls and neither reached the record.
* **7 differed from one run to the next** and 11 did so at least once across the seven.

**The 104 were the damage.** `git checkout -- richos/app/ui/tests/shots-*` after a run was the
standing habit and every one of those reverts threw away a legitimately updated reference. A
reference that is dirty after every run trains everybody to discard it and a reference nobody
trusts protects nothing — which is the whole reason the run-to-run half matters at all.

**Three things were still moving when the shutter opened and each is now waited for by a
stated condition rather than a duration.** All three are in `lib/harness.js`, with the
measurement in the comment that fixed them:

* **the pointer, left where the last click put it** — `contrast.js` drives forty-two surfaces by
  clicking them and where the new state puts a different control under that point the control
  is photographed hovered (`style.css:1628`). `voice-model-progress.png` alternated between a
  hovered and an unhovered `Stop the download` on every run, 1,200 pixels at delta 109.
  `parkPointer` moves the pointer out and waits for the **hover chain itself** to agree with
  where the pointer now is. Opt-in: a suite that photographs a hover on purpose keeps it.
* **a three-second background poll** — `main.js:3098` re-renders the work chip from
  `get_worker_status` on every tick, so a shot between a driven state change and the next tick
  shows the step before it (`shots-26/ms-05` read `· 1 I can't see` in one run and `· 1 done ·
  1 I can't see` in the next, 962 pixels at delta 131). `awaitWorkerChipSettled` waits for the
  product's own re-render to agree with what is already on screen.
* **a live counter counting the harness** — `main.js:1437` ticks the elapsed time once a second
  while a row is live, so §26's `Working for 18s` had become 34s in one run and 35s in the next
  and five of nine shots moved for no reason but how long the walk took. `pinClock` pins the
  page's wall clock for the length of the capture and recomputes the row through the product's
  own visibility handler (`main.js:1446`) — no re-implemented formatter, no waited-out tick.
  **The band above the composer sat outside that handler until 2026-09-20** and went on
  counting this directory's own runtime inside an otherwise pinned frame; it is recomputed
  there now, which is what `startOrStopWaitTimer`'s own comment always claimed.
* **a countdown whose sentence changes one second into the page's life** — the phone pairing
  window is 300 s and the harness's clock starts when `mock.js` loads, so
  `ceil((300000 - elapsed) / 1000)` is 300 for the first second and 299 after it and
  `remaining()` floors those to `5 more minutes` and `4 more minutes`. Measured 2026-09-20: the
  sheet is reached 879–1,055 ms after `goto`, straddling that boundary and
  `shots-phone/phone-{light,dark}.png` and `shots-contrast/phone-pairing.png` each came back
  different under concurrent load — 775 pixels, delta 164, inside one 120x13 box holding that
  sentence. The fixture's own `phonePairingSecondsLeft` knob pins the ANSWER (it does not move
  the clock) at 270 s, which is `4 more minutes` with thirty seconds of margin to either edge
  of its minute. Only the pages that are PHOTOGRAPHED take the pin; the checks that read a live
  countdown keep the running clock they were written against.

**And that three-second poll is now waited for by every committed shot of the conversation
surface, not only §26's.** The poll used to have **no leading call** — the interval was its only
caller — so from the moment the app opened a conversation the chip zone was empty for up to
three seconds and then a whole line of text arrived in it. Every shot in `techy.js` (10) and
`updates.js` (9) was being taken inside that window: all nineteen changed when the wait went
in and `3-1-03` had alternated between two pictures 6.01% apart under concurrent load. Both
suites now shoot through a local `evidence()` that calls `awaitWorkerChipSettled` and throws
rather than photographing a surface that is still moving — the same shape `memory-strategy.js`
has used since 2026-09-19.

**The leading call landed on 2026-09-20 and the waits stay.** `openThread` now awaits
`pollWorkerStatus()` before it returns, so the chip is on screen with the conversation and the
product-side hole is closed. The `evidence()` waits are NOT removed: the interval still
re-renders the chip for as long as the page is open, so a shutter can still land mid-re-render
— what changed is the price. Measured on one host, serially, same tree, both runs on a host
with no nightly build on it: `techy.js` 104 s → 65 s and `updates.js` 78 s → 50 s and
`work-summary.js` 18 s → 7 s WITH a check added to it, because its own waits on the chip's text
were paying the same three seconds. Whoever deletes those `evidence()` waits on the strength of
the leading call will get the alternating picture back.

### AND A TEST RUN NEVER WRITES ONE AT ALL

Since 2026-09-29 a changed picture is **announced and kept, never written over its reference**.
The merge gate verifies a candidate in the main checkout; `splash.js` photographed two app
screens the candidate had legitimately changed, wrote them over `shots-splash/splash-03` and
`splash-04` and the gate refused the merge as "source changed during verification" with every
check canceled. The nightly had been doing the same with `home.js` and restoring it afterwards.
A test that rewrites the tree it is verifying is making an unreviewed commit, so:

* the run prints `shot changed: <file> — <difference> — the committed reference is NOT
  rewritten`, keeps this run's picture at `.shots/changed/<same path>` (gitignored) and names
  the exact command that accepts it. A changed picture is still not a failure, as before;
* **and its evidence outlives the checkout** (2026-10-20): its actual picture, the reference,
  their difference (`diff.png`, the reference dimmed with every differing pixel in magenta) and
  the page's state at the shutter (every scrolled box and its offsets, the focused element, the
  viewport) are kept in `.shots/evidence/<picture>/` and, under `proof-run.py`, in the check's own
  results folder in the run's evidence (`RICHOS_TEST_RESULTS_ROOT`), which a merge gate's
  temporary checkout does not take with it;
* a reference is written only under `RICHOS_SHOTS_REGENERATE` (below), in your own worktree
  and committed with one line saying why it changed;
* **every suite's report now carries the check "the tracked tree is exactly as this suite found
  it"** (`lib/tracked-tree.js`): the harness takes `git status` when a suite starts and again
  when it reports and a tracked file whose content changed or a new untracked file under this
  directory, is a FAIL in that suite's own report, naming the file. So the next writer — one
  that never goes near `publishShot` — is red on its own branch, by name, instead of refusing a
  merge gate later. `tracked-tree.js` proves both halves, red and green.
* **what that check watches is the UI tree, `richos/app/ui`, not the whole checkout** (part-2
  hunt section 07, 2026-09-29: one editor save anywhere in the repository failed every UI suite
  running at that moment). The design choice: ONE shared scope, the tree where UI suites write
  on purpose, instead of each suite declaring what it may write. Every writer found is inside
  it (`publishShot` into `shots-*`, `run.js` into `suite-weights.tsv`), so the catch stays and
  no suite needs a declaration; the cost is one scope function in `lib/tracked-tree.js`. A save
  outside it is no longer blamed on a UI suite; a write into it, by anyone, still is.
  `tracked-tree.js` check 6 proves both sides. Under `proof-run.py` a UI suite has no reviewed
  input contract, so it is keyed by the whole checkout: any change during the run invalidates
  its pass and it runs again, but it is never failed for that and never stops or invalidates
  another check. Narrowing that needs a read declaration per suite, which is separate work.

### AND WHAT IS LEFT IS DECLARED, WITH ITS CAUSE, ITS MEASUREMENT AND ITS BOUND

`lib/shot-stability.js` is the only place a committed shot may differ without being rewritten.
It names **one file at a time**. There is no global tolerance and there will not be one: a
global tolerance is a lie told about every file at once, while a declaration is a claim about a
single file that says what is moving in it — and a claim can be checked, argued with and
removed.

`samePicture` is **not** loosened. It stays exact, because it is the primitive everything else
rests on; the bound is a separate, named layer applied by `publishShot`, so *"these two are the
same picture"* and *"these two differ by less than I have declared for this one file"* stay
different sentences.

**A held shot is never silent.** Every run prints the file, the class, the measured difference
and the bound it was held under, exactly as loudly as it prints a shot that changed:

```
  shot held: shots-home/home-named.png — the live field, running — 291789/1296000 pixels
    (22.5146%), worst channel delta 245 — inside its declared bound (at most 35.0% of pixels)
```

That line is the only thing standing between a bound and a blindfold. Read the numbers in it,
not just the file names.

| Declared | Class | Measured, run to run | Bound |
|---|---|---|---|
| `shots-home/home-named.png` | the live field, running | 6.08%–24.90% of pixels, delta 234–253 | 35% of pixels |
| `shots-home/home-returned.png` | the live field, running | 14.08%–25.14%, delta 239–247 | 35% of pixels |
| `shots-home/home-anonymized.png` | the live field, running | 6.44%–17.77%, delta 205–248 | 35% of pixels |
| `shots-splash/material-round-11-v1.png` | compositor dither | 0.07%–0.59%, delta **2** every time | delta ≤ 2 |
| `shots-splash/material-round-11-v2.png` | compositor dither | 0.17%–0.77%, delta **2** every time | delta ≤ 2 |
| `shots-contrast/inspector.png` | compositor dither | unchanged in three of seven, else 0.52%, delta 2 and once 3 | delta ≤ 3 |

**Two rows left this table on 2026-09-20, because what they blamed was not what was moving.**
`shots-26/ms-04` and `ms-05` were declared at 79 and 116 pixels against a `startedAt` that
`mock.js` was said to stamp from the real clock at §26.12. Then `ms-03` — photographed six
steps before that run exists — moved in the same 10x15 box at x[1162..1171], `25s` against
`26s` and nothing in `mock.js`'s `memoryStrategy` reads the wall clock at all. The element was
the **wait band above the composer**: `pinClock` reaches the surface through the product's own
`visibilitychange` handler and that handler recomputed the timeline's timers and not the
band's, which runs its own one-second `renderWaitBand` off `Date.now()`. `main.js` now
recomputes the band there too and starts or stops its timer — which is what
`startOrStopWaitTimer`'s own comment claimed and what §6.2 requires of both clocks. Five
consecutive same-source runs after that, three of them with five suites running concurrently,
left all nine §26 shots byte-identical, so there is nothing left to declare and nothing is
declared.

**The live field cannot be frozen into a byte-equal shot and that is measured rather than
assumed.** `home/field-engine.js:1031` integrates 7,500 objects on springs and flies packets
along 12,817 links off `performance.now()`; its randomness is seeded (`mulberry32(6401)`) and
there is no `Math.random` anywhere in it, so the wall clock is the whole of it. A test-side
frame clock — `performance.now()` replaced by a counter advancing one 60Hz step per animation
frame — was built and measured on 2026-09-05 and took two loads paused at the same frame from
44,039 differing samples to **293**, not to 0: the blur and glow framebuffer passes are
floating-point and order-dependent on the GPU. It would buy a smaller envelope at the cost of a
clock override across three large suites and would still need a row above.

**The dither bounds are on the DELTA, not the area and that is what makes them checks.**
`splash-01-round-11-v1.png` drifted for a day in September 2026 at 88.46% of pixels and delta
**63** — a photograph of the home screen through a curtain at opacity 0 — and nothing failed,
because check 5's guard asked only whether `#splash` was still in the DOM. A delta bound fails
that on the spot; an area bound would not.

**`shots-26`'s two entries are a fixture gap, named rather than bought off.** §26.12 opens a
second Sage run and `ui/mock.js` stamps its `startedAt` from the real clock, so one
right-aligned counter reads `31s` or `32s` depending on how fast the walk got there. Anchoring
that start the way the first one is anchored (beside `setMemoryStrategyAnchor`, `ui/mock.js`)
deletes both rows.

Everything else in `shots-*` is byte-identical across consecutive runs, so a diff in it is
signal.

**To regenerate deliberately** — the ONLY way a committed shot is written. Name the pictures that
failed: the failure prints the exact command naming each of them and a file named on its own is
written whenever the picture differs at all. A directory or `all` is a sweep: it writes every
picture that differs EXCEPT one held inside its declared bound above, so a noise-only redraw of a
picture nothing changed is not kept (name the file to rewrite one of those).

**A regeneration run fails on purpose** ("no picture was approved by the run that drew it",
2026-10-20): the run that drew a picture is not evidence that it is right. Look at each one — its
previous reference, the new picture, their difference and the page's state are kept in
`.shots/regenerated/<picture>/` — commit it in your own worktree with one line per picture saying
what the look found, then run the suite again without the switch. That run is the evidence.

```sh
RICHOS_SHOTS_REGENERATE=shots-home/home-named.png node home.js   # the failed pictures, by name
RICHOS_SHOTS_REGENERATE=shots-home node home.js                  # a sweep of one directory
RICHOS_SHOTS_REGENERATE=all node contrast.js                     # a sweep of a whole run
```

**`splash-01-round-11-v1.png` is where the canary story comes from and it no longer needs a
row.** Its signature was 20.7% of pixels at worst channel delta **1** — dither, nothing moved.
For a day in September 2026 it drifted 905,792 of 1,024,000 pixels (88.46%) at delta **63** and
nothing failed, because check 5's guard asked only whether `#splash` was still in the DOM. It
was: the ceiling had fired, the fade had finished and the node had 40 ms left before removal,
so the suite filed a photograph of the home screen through a curtain at opacity 0 and called it
green. It measured byte-identical across all five runs on 2026-09-19, so it is not declared —
and if it ever drifts again it will be announced as changed, with this run's picture kept in
`.shots/changed/`, which is the one thing a reference is for.

## When a page load fails: the navigation evidence bundle

On 2026-09-21 one release run failed `contrast.js` on a single `page.goto: Timeout 30000ms
exceeded.` at `entity-view/dark` — a `file://` load that normally takes under a tenth of a
second. The isolated rerun passed and the failed run had left nothing behind but Playwright's
two-line call log, so the cause is still unknown. **If it happens again, the run now writes down
what the page was doing.** The rule it follows is the one the record set: capture the failing
state; never retry, quarantine or widen a deadline.

**Every suite gets it without doing anything.** `loadPlaywright()` installs
`lib/navigation-evidence.js` on the browser type, so every page any suite opens has an observed
`goto` and `reload`. The deadline is the one the suite asked for (its own `timeout` or the
page's default), the arguments pass through untouched and the original error is rethrown
unchanged, so the FAIL line is the same one a suite printed before. A navigation that succeeds
writes nothing and prints nothing.

**Where the bundle lands:**

| How the suites were started | Bundle directory |
|---|---|
| The nightly's UI gate or anything passing `--receipts=<dir>` | `<dir>/navigation/` — beside the receipts, the directory the gate names when it refuses a build (`~/.richos-nightly/ui-receipts/navigation/` by default). Emptied at the start of each `--shards` run, like the receipts. |
| `npm test`, `node run.js`, `node run.js --shards=N` without receipts or one suite by hand | `.shots/navigation-failures/` in this directory (gitignored) |
| `RICHOS_UI_NAV_EVIDENCE_DIR=<dir>` set | `<dir>`, always |

The run's output names every bundle as it is written, on the failing suite's stderr, before
that suite's PASS/FAIL report. Its shape, with illustrative values:

```
  navigation evidence: <dir>/contrast-12345-1.json
        goto file:///…/ui/index.html failed after 30001.2 ms; reached request, response, commit, domcontentloaded; not reached load; 1 request(s) in flight
```

**How to read one.** `<suite>-<pid>-<n>.json`, plus `<suite>-<pid>-<n>.png` when the page could
still paint. In the order to read them:

- `check` and `page.context` — which check it was (the surface) and the `colorScheme` the page
  was opened with (the theme). `dom.dataTheme` / `dom.storedTheme` are the page's own view, when
  it could still answer.
- `elapsedMs` against `options` — when the navigation gave up, against the deadline it was
  given. `error` is Playwright's own message, verbatim.
- `reached` and `lifecycle` — which of request, response, commit, `domcontentloaded` and `load`
  happened, each with its time in ms from the start of the call. Anything marked
  `afterFailure: true` arrived while the evidence was being collected: a `load` there means the
  page was slow, not stuck.
- `requests.inFlight` and `requests.failed` — what the load was still waiting for, with status
  where a response came back. For a `file://` load, `dom.referenced` and `dom.resources` say
  the same thing from inside the page.
- `console` and `pageErrors` — errors and warnings during the navigation.
- `dom.unavailable` or `screenshot.unavailable` — the page could not answer an evaluate or
  paint within its bound (2 s and 3 s). Both together mean the page's own main thread was busy
  or wedged, which is a finding in itself.
- `process` and `host` — how many navigations this suite had made, the last ten passing ones
  and their durations and the machine around the failure: load average; `cpuBusyPercent`, one
  second of the kernel's own CPU tick counters read in the suite's process (the same busy
  split `scripts/testvm/reserve.py` admits heavy work on, with `cpuSampleMs` saying how long
  the second really took); the kernel's `memoryPressure` verdict (normal, warn or critical)
  and `memoryAvailablePercent`; and the ten busiest processes from one `ps` snapshot. The CPU
  sample starts no other process, so a host too loaded to start one still gets its number
  (nightly `20260929T044015Z-588457c3` lost it to `spawnSync /usr/bin/top ETIMEDOUT`); the
  memory verdict and the process list are best-effort and one that could not be read says so
  in `memoryUnavailable` / `processesUnavailable`. `freeMemBytes` is NOT a pressure reading:
  on macOS it leaves out the inactive memory the kernel reclaims on demand, so it is low on a
  healthy machine.
- `commit` — the checkout the suite ran from.

**What it costs.** On a passing run, a few event listeners for the length of each navigation:
`node lib/navigation-bench.js 100`, run twice on 2026-09-22, measured the median load of
`index.html` at +0.711 ms and +0.336 ms with the capture (about 38 ms either way) and the p95
difference at -3.434 ms and +4.588 ms — noise. `navigation-evidence.js` check 6 prints the same
comparison on every run. On a failure, about two seconds of collection after the navigation has already failed (most
of it the one-second CPU sample) and up to about eight for a page too busy to answer — bounded,
so a wedged page cannot turn it into a second hang — and at most five bundles per suite
process. `navigation-evidence.js` proves all of this against a local server that never
answers, including that a failing suite stays red. `RICHOS_UI_NAV_EVIDENCE=off` removes the
instrumentation entirely, for measuring it.

## What is here

| File | What it proves |
|---|---|
| `rich-files.js` | A FILE FROM RICH (round 36 screen 3, §138 item 12): the files Rich puts in front of the user are cards under his words, inside his last message of the turn; a click anywhere on a card or Enter on it, asks the shell to open it by its attachment id (`rich_file_open`, resolved by `opener.rs`) and Show in Finder asks `rich_file_reveal`; a file attached between turns arrives live on `rich://rich-file`; a gone file draws the dashed card, "No longer on this Mac. It was moved or deleted." and Ask Rich again, which sends "Please send <name> again." and opens nothing. Text, meta, Open in … and the card edge are computed against WCAG AA in both themes. Writes `shots-36/files-{ready,missing}-{dark,light}.png`. |
| `questions.js` | Round 13 cards preserve the composer, require explicit multiple-choice submission, support revision edits and retry the same saved answer. Both themes meet text and control contrast thresholds. |
| `phone.js` | USE RICH FROM YOUR PHONE (`ui/phone.js`, phone-client plan §4.1) and since **CEO §61** there is one path through it: the phone reaches this Mac over the user's own Tailscale network, from anywhere. The route chooser, the self-signed certificate, the trust page and the sixteen certificate taps are gone from the product and checks 4, 5 and 6 are now about what replaced them. The shipped QR encoder is proven identical to `tools/phone-probe/lib/qr.js` — the copy that is checked against ISO/IEC 18004 Annex I and against Apple's own Vision decoder — module for module, so the port cannot drift into being wrong in a second way; a too-long URL throws rather than drawing something unscannable; the code is drawn square at the size its own content needs, with a tailnet address written out beside it. **The canvas is measured FROM THE PIXELS with `getImageData` in both themes** — exactly two colors present, opaque #000000 and opaque #ffffff at 21.00:1 and the quiet zone white on all four edges plus the corner. That check is what `contrast.js` check 14's canvas exclusion names and without it that canvas would be unmeasured by anything. Then the prose, which is the feature: the screen says nothing is installed on the phone and names the one thing that would mean something is wrong; the sheet opens on the flow with no question in front of it, walked on three Macs (no Tailscale, signed in with no name yet, ready); the four phone steps are four and none of them installs anything; the six words are six with what to do when they do not match; a live code says how long is left and an expired one shows no code at all; and the paired card says exactly what forgetting does. Writes `shots-phone/`. Mutations listed at the foot of the file. |
| `no-home-network.js` | **NO HOME-NETWORK PATH IS EVER BUILT FOR THE PHONE AGAIN** and this is the mechanism rather than the memory. The CEO, 2026-09-19: *"How many more times will any 'home network' related shit be associated with a phone or built for phone?"* — asked after a build kept the route chooser, the certificate and the sixteen taps through §61 and after briefs asserted §61 kept both paths. Four surfaces are scanned — `ui/phone.js`, `src-tauri/src/phone/**`, `src-tauri/src/main.rs` and `web/web-app/**` — for eleven strings a reintroduction would have to write: `at home` / `AtHome`, `home network`, `home plan`, a `.local:8443`/`:8444` pairing origin, the trust port, `trust QR`, `self-signed`, `certificate profile`, `Remove Profile` and `sixteen`. **Comments are stripped first**, because the record of the ruling is written in comments and a check that could not tell a ruling from a violation would give a false positive as its first correct answer; the stripper knows a template literal's markup comments from its strings and a Rust lifetime from a char literal, both of which cost a false positive on the first run. **Check 5 is the positive control** — every banned string planted in code position and every one required to come back, because a negative result cannot tell *absent* from *the check could not see it*. Check 4 is the negative control: a comment recording §61 must pass. Check 6 asserts this suite's own header, which names all eleven, is not a violation of what it enforces. |
| `provider-auth.js` | Browser sign-in, cancellation, account selection and reopening a saved connection. |
| `accounts.js` | Claude accounts for someone who is not technical (round 18, the CEO's feedback item 9 of 2026-10-06): the Settings row for everyone with its plain line, the account cards in the order Rich uses them, Use this one now with Undo, adding a second account in two steps (with the same account signed in again as its own screen), why Rich switched in the conversation, the sheet's banner and Recent changes, Rich's one-time suggestion with Not now, the both-near banner, the 5-hour choice and Remove. Every state in both themes at 1440 by 900 and 1024 by 700: AA contrast and the type floor computed, nothing sideways, every control reachable, no m-dash or n-dash. |
| `quota.js` | The Claude Code quota panel in Technical settings (round 14): the five-hour, weekly and per-model windows with their reading age, the 93% pause threshold field with Save and Keep, held-agent status, stale and unreadable readings shown as their own state and the weekly free-reset offer with its advance approval. Fixture numbers only; both themes. |
| `repositories.js` | Explicit company selection, two repository connections and retry after a refused folder. |
| `work-summary.js` | Saved work receipts render separately from live-worker liveness. |
| `background-work.js` | THE BACKGROUND-WORK SURFACE (the background-work spec, revision 5, in the private richos-hq record; §0 rows 6 and 7, §3.5, §4.2, §7.8). The sentence IS the feature: since the CEO's ruling §52 (2026-09-18) a job lands on its own, so a `blocked` assignment reads "waiting for your decision before it can go on", makes no claim about his repository (it may reach such a step AFTER landing something) and contains none of done/finished/complete/landed — and the same scan is re-run against the `settled` row, which DOES speak of finishing, so a clean result is a fact about the blocked row rather than about a scan that cannot fire. The stop control is one per OPEN assignment and names its own by accessible name ("Stop landing the three branches"), because Stop stays the conversation's and a bare "Stop" beside three of them is a control he cannot use without counting rows; pressing one carries that assignment's id and no other. Contrast is measured from WebKit's own resolved colors, alpha-composited against the real ancestor background, in BOTH themes — dark 12.06:1 / 5.78:1, light 18.07:1 / 6.38:1, every node at 16px, nothing declared exempt — and `openPage` proves the shipping stylesheet and the shipping renderer both loaded before any of it is read, because a missing stylesheet would report the browser's defaults and pass. The held-notice half is read from `main.js` rather than driven (running a real turn needs a live bridge and a mocked one would be asserting about the mock), with a positive control: the same patterns re-run against a source with the hold removed must fail. |
| `quit-question.js` | THE QUIT QUESTION (the background-work spec, revision 5, §2.5, §2.5a, §7.4a). Closing the window no longer quits, so this sheet is what he sees when he chooses Quit with work running. The shell has ALREADY prevented the exit by the time it renders — the runtime reads the exit answer synchronously — so the suite pins that the safe answer is the focused one, that Escape ANSWERS rather than dismisses and that "Keep working" reaches `cancel_quit` and nothing else, with the quit control proven able to reach `confirm_quit_and_stop` as its positive control. The completion-claim scan is on phrases rather than words, because the sentence that makes quitting safe to understand ("everything it has done so far is kept") contains the word a bare scan would have failed on; that scan carries its own positive control. Contrast is measured from WebKit's own resolved colors in BOTH themes — dark 5.78:1 title and question, 7.68:1 on the quit control; light 6.38:1 and 4.72:1 — every node at 16px or more, nothing declared exempt. |
| `instructions-update.js` | IMPROVED RICH AND STU INSTRUCTIONS (CEO, 2026-10-22; built as drawn from richos-hq `design/mockups/rounds/round-37/`). The question appears only when the shell names a changed file, in his words, with each file and where it is and OK focused; No and Escape reach `decline_instructions_update` and give the focus back; OK updates the files one after another (Updating, Next, Done) through `update_instructions_file`, then opens the before and after: one tab per file with its count of edits, the update's additions and the edited-out text marked, the user's own edited-out lines tagged "your line", the edits stepped through by the arrows, Option-arrows and the map, the file opened or shown in Finder through `instructions_file_open`, Done closes. A refused update is said with a way out. Contrast from WebKit's resolved colors in BOTH themes, translucent highlights composited: question 12.06:1 dark and 18.07:1 light, OK and Done 7.68:1 and 4.72:1, added text 11.32:1 and 12.93:1, edited-out text 11.78:1 and 12.88:1, every node at 16px or more, nothing declared exempt. |
| `attachments.js` | SCREENSHOTS AND FILES ON THE MAC COMPOSER (CEO §86, 2026-09-24): the thing the terminal gave him that the window did not. Through the real shell: a pasted screenshot becomes a chip with its name, type, size and a preview and Send hands Rich his words followed by the phone's own block (`Attached on this Mac (1 file, saved by RichOS):` and one line per file); a dropped PNG and PDF, one removed by keyboard with focus kept in the tray and only what stays is sent, with no words at all; every refusal (a zip, a folder, a disguised PDF, an empty file, 25 MB and one byte, an eleventh file) is a named sentence under the composer and the oversize paste never crosses the bridge; the files belong to the thread they were dropped on; a file the Mac no longer holds leaves the tray by name and nothing is sent; a new thread's first message carries its files into the thread that send created; the drop target shows only while a drag is over the window. Check 7 reads `phone/attachments.rs` and `mac_attachments.rs` and asserts every sentence, the heading and all 13 accepted kinds are identical in `mock.js` and `attachments.js`. Contrast is computed from WebKit's resolved colors in BOTH themes over the rendered tray, refusal line and drop target: 0 failures and the three non-text indicators measured at dark 4.57 / 7.68 / 5.78 and light 3.35 / 5.48 / 6.38 against 3:1. The bytes reaching the conversation's folder are the Rust suite's (`mac_attachments::tests`); Rich describing them is the VM walk in richos-hq. |
| `local-notice.js` | A LOCAL NOTICE IS THE APP TALKING ABOUT ITSELF, NEVER AN ANSWER FROM RICH (Ray's candidate-.4 finding #8). A voice notice used to be written into the model as `rich_message`, so the renderer stamped it with Rich's byline and avatar and offered **Copy Rich's message** — reading back, a microphone status line was indistinguishable from something Rich said and only one of the two is in the ledger. The notice now has its own kind and its own presentation and this suite renders one BESIDE a real Rich message on the same page: the notice has no byline, no avatar and no control, the message has all of them, so each absence is a fact about the notice rather than about an empty page. A screen reader is told "RichOS status" where a message says "Rich said"; the sentence itself renders verbatim, because this moves a line out of speech rather than editing one. Contrast is computed from WebKit's own resolved colors in both themes — body 12.06:1 dark / 18.07:1 light at 16px and the 2px left rule that marks it 6.54:1 / 6.58:1 against the 3:1 non-text floor. |
| `operator-notice.js` | HIS TEAM, ON THE CONVERSATION'S TIMELINE (the operator-client record's §7 item 1, in the private richos-hq record; `app/STREAMING.md` "His team, on an operator install"). Each of the seven kinds (`update`, `question`, `answer`, `outcome`, `failed`, `alarm`, `team`) is drawn as the local-notice card with a from-line saying who it is from, beside a real Rich message that has the byline and Copy the cards lack. The attribution is only claimed where it is true: `team` is RichOS speaking about his team ("About your team") and `alarm` is his engine's, so neither says "Your team", an `outcome` never says done and an unknown kind is still drawn; the five lead kinds DO say "Your team", which is the positive control. In the shell (the mock's `operator: true` preset): notices said while he was on another conversation are drawn when it opens, between the two answers they came between, with the assignment's title and never its identifier, marked told and not drawn again on a second opening; a pushed notice is drawn at once; one pushed mid-turn and a background result read mid-turn, are held and drawn at THAT turn's end (this check found that the shipped flush never fired at a turn's end). One report arriving on both lanes with the same words is one attributed card in either order, while two assignments saying the same sentence stay two (driven through `main.js`'s own functions, as `background-work.js` does). On a product install the read is asked and answers nothing, no team card or from-line is drawn and a background result still reads "RichOS status" — with an operator install as the positive control. Contrast is computed per kind in both themes: from-line 5.52:1 dark / 5.35:1 light, body 12.06:1 / 18.07:1, all at 16px; left rule 6.54:1 / 6.58:1 (`--attention`) and 5.84:1 / 8.12:1 (`--danger`) and the two tones are asserted to be two colors. |
| `workers.js` | §25 "AI workers", criterion by criterion, against the renderer in isolation. Includes the delegated-worker regression before/after and the cross-entity negative control. |
| `live-workers.js` | The LIVE half of §7, plus §6.4's disclosure. Every worker chip in it arrives through `RichTimeline.onWorkerUpserted` and never through a snapshot — the path `rich://worker-upserted` opened on 2026-08-29. Proves the live DOM and the reloaded DOM are byte-identical, that an `activity` row upgrades to a `worker_activity` row under the same id without stale fields, the cross-entity negative control on the live payload and §6.4's two defaults with the CEO overruling both. |
| `inspector.js` | §7.2 the read-only inspector, §7.3 the background-work summary and §20's three breakpoints — through the REAL shell (`index.html` + `main.js` + `mock.js`). |
| `output.js` | THE OUTPUT PANEL, slice S4 of the Output side panel PRD (§12.4 of the 2026-10-05 PRD, kept in the private richos-hq record) — the CEO's *"a button here … at the top and at the bottom … this entire side panel would open … with a list of all the files … that were output by this entire thread"*. Through the real shell on `mock.js`'s `{ output: "round-17" }` preset: round 17's states `closed`, `open`, `empty`, `from-link`, `from-digest` and `arrives` each reproduced and named; both buttons and their count, grouping and order, row → file → back with the row kept, Escape one level at a time, ⌘⇧O, focus in and back, the inspector and the panel never both open, a missing file, an unreadable record, no button without a thread, the overlay below 1180px, reduced motion. Every new text pair and indicator computed in both themes; the clock is fixed so `shots-output/output-{open,empty}-{dark,light}.png` do not move with the hour. |
| `output-truth.js` | THE OUTPUT PANEL SAYS WHAT IS TRUE NOW — the four Output panel defects Ray's walk of nightly candidate 38 found (`docs/verification/2026-10-05-nightly-38-candidate-walk.md`), each red on `ed13a8c76` and green with its fix, through the real shell on the round-17 fixture. D6: a file deleted while the panel was closed is marked missing the moment it opens again (row dimmed, the head counts it, Open and the menu off with the §6.7 sentence, its view never "Written … · size"); a conversation link to such a file opens it as missing; a file gone while the panel is open is said gone when shown and its row dims. D7: Under the hood closes the panel and both Output buttons read unpressed; ⌘⇧O from there closes Under the hood; one press reopens. D9: the CSV facts line, read glyph by glyph at four widths, never starts or ends a line with "·". D10: the panel computes `color-scheme` dark in dark and light in light, so its scroll bars follow the theme. |
| `realbytes.js` | The join the other suites cannot make: the payload `cargo run --example timeline_payload` prints from a real ledger on disk, rendered by the real renderer. Catches field-name and shape drift between backend and UI. |
| `memory-strategy.js` | §26's sixteen-step fixture, driven end to end through the REAL SHELL by typing the prompt and pressing Enter. Injected clock (the two-hour turn runs in under a millisecond), the nine required screenshots and the negative half: every §26 step this runtime cannot produce is asserted ABSENT. |
| `affordances.js` | THE RULE: a state the user could change must render the control that changes it. The state inventory is derived from source every run (`lib/state-strings.js`), classified in `lib/state-registry.js` and the two sets must be EQUAL — a new user-visible string that nobody has classified fails the suite. Every ACTIONABLE state is then driven up in the real shell and its control asserted present, visible, enabled and interactive. Carries a negative control (the scrape examined 112 states, not zero) and four positive controls (a missing, a hidden and a fake control and an unclassified state, must each be flagged). |
| `lib/state-strings.js` | The derivation. Comment-stripping scanners for JS and Rust, HTML text nodes and human-readable attributes, `+`-concatenation folding and four named blind spots. `node lib/state-strings.js` prints the inventory with file:line. |
| `lib/state-registry.js` | The classification, one row per state, each with its reasoning. Annotation only — the inventory above is the authority. |
| `lib/harness.js` | WebKit launch, the fixture page, pixel-verified screenshots, the four-line runner and the shutter's own stated conditions — settled animations, pinned loops, a parked pointer, a caught-up work chip and a pinned wall clock. |
| `lib/shot-stability.js` | WHICH committed shots may differ from run to run, by how much and why — one file at a time, each with its cause, its measurement and its bound. Self-tests at require time: every declared file must exist, every entry must carry a bound and a reason and the arithmetic is probed symmetrically so a bound that held everything would fail here. |
| `lib/navigation-evidence.js` | On a `page.goto` or `page.reload` that fails, writes the evidence bundle described in "When a page load fails" above, then rethrows the original error. Installed on every page by `loadPlaywright()`; changes no deadline and no result. |
| `lib/fixtures.js` | Timeline payloads in the exact shape `get_timeline` puts on the wire. |
| `steering.js` | §25 "Steering and stop", criterion by criterion, through the real shell. The `You stopped after {duration}` row from the real wire bytes, the crash that is never attributed to the CEO, a stop that reached nothing saying so and the stop control at §20's three widths. |
| `second-mouth.js` | A MESSAGE HE DID NOT TYPE HERE — Ray's candidate .13 defect R3. This window drew the CEO's own sentence itself, optimistically and therefore never needed to be told what he said; his phone is a second mouth and made that assumption false, so a message typed there took **5.68 s** to reach the Mac (measured twice) and arrived in the same frame as Rich's finished answer, against 0.096 s the other way. The window is handed the exact event sequence `spine.rs` emits for a phone message — `turn-status: queued`, `rich://ceo-message`, the sidebar row — with NO composer interaction and his sentence has to be on screen before Rich has written a word. The negative control withholds `rich://ceo-message` and nothing else, which is `a2cef8ee` exactly: the status events arrive, the rail moves, the thread stays blank. Plus the two things a new listener can break — his own typing still renders exactly ONE bubble through the optimistic path, the event and the reload and an event fenced to another company never reaches this screen. |
| `restart-scope.js` | What happens BETWEEN threads and what survives a restart. A working thread stays visibly active while another is selected and its timer resumes rather than restarts; drafts and scroll positions belong to one thread and never cross an entity; a turn streaming elsewhere renders nothing here, across entities and inside one; the fence is on every live handler, with the inventory derived from the shipped object and cross-checked against `timeline.js` on disk; duplicates render once; missed events recover from the snapshot; an in-flight turn survives a restart as unknown; a mid-turn crash draws the CEO's prompt once. |
| `corrections.js` | THE CORRECTION DESK (§7 "ask, never infer"), both families, through the real shell — RICH-TODOs row 5b. Confirm reaching the Rust desk and nothing written before it; the preview asserted byte-identical to the writer's own `--dry-run` output; a plain decline suppressing nothing and staying re-askable; a permanent decline landing on a visible list that lifts; an absent desk stating its reason instead of rendering an empty list, with the present-and-empty case as its positive probe; four refusals relayed verbatim to the screen rather than a console; the fourteen registered commands joined to `main.js` and `mock.js` on disk. Writes `shots-5b/`. Every check run RED once — `docs/verification/correction-desk-2026-08-30/mutation-runs.txt`. |
| `feedback.js` | THE FEEDBACK CHANNEL (`feedback.rs`), through the real shell — RICH-TODOs row 5. The rail control that never counts up at him and the absence of any trigger, asserted structurally; the question and four keys byte-identical to the Rust constants; the offer raised on 1 and 2 and on nothing else; a dismissal recorded and a closed panel recorded as nothing; the whole vocabulary on screen with ZERO free-text fields; the preview compared byte for byte against what `render_disclosure` really produces, the same terms in reverse rendering the same bytes and the whole report visible without a nested scroller; an approval that records the terms and no prose; an approval for ALTERED text refused with its positive control beside it; four unrecognized keys refused rather than taken as dismissals; the store-would-not-open, read-refused and genuinely-empty states as three different facts; 40 answers in one column. Writes `shots-5/`. Every check run RED once — `docs/verification/feedback-channel-2026-08-30/mutation-runs.txt`. |
| `scale.js` | THE TWO PROMISED NUMBERS in §25 "Accessibility and performance" — RICH-TODOs virtualization row. 10,000 items through the real renderer in real WebKit: one new activity row costs 24ms (2ms of it JS) against a 257ms baseline, 999 of 1000 turn sections are asserted IDENTICAL BY REFERENCE and the reused DOM is compared byte for byte against a from-scratch render. Plus the drag over the whole 518,189px history asserted to do NO work — zero DOM mutations AND zero reads of the model, both instruments carrying a positive probe, because turn reuse makes a re-render-on-scroll regression invisible to a DOM counter alone — and the same motion at 1,000 turns against 120 costing no more per frame, matched on one viewport of fresh content per frame so neither arm re-reads warm tiles. The absolute 60fps number is measured and printed on every run and gated only by `RICHOS_FRAME_BUDGET_MS`, because on a GPU-less runner it measures the runner. Plus the scroll-container probe, the projection budget set BELOW the baseline it catches, the steering predicate against the brute-force scan it replaced, `turnRecord`'s order invariant across every mutation site, NO PAGINATION asserted structurally (every turn mounted, no page control) and a join to the Rust test that owns the OTHER 10,000 (the entity index, `richos/app/src-tauri/src/main.rs`). Numbers: `docs/verification/timeline-scale-2026-08-30/`. Every check run RED once — `mutation-runs.txt` there, including the four that did not go red the first time. |
| `splash.js` | THE OPENING SCREEN and its off switch, through the real shell. The variation library asserted to be DATA — the file is stripped of its one assignment and handed to `JSON.parse`, so it cannot contain code — and the renderer asserted to hold no colour literal, so a new variation can only ever be a new entry. Every shipped entry forced and drawn in full; twelve real launches producing distinct compositions with zero immediate repeats; the palette study's chips, hexes and corner labels asserted ABSENT by element inventory; the whole curtain click-through with a centre click reaching the app beneath; the ceremony cut but the mark pinned rather than left as unlit steel; three unusable-library shapes each producing a normal launch and no half-drawn frame; a launch that never reports ready still clearing on its own ceiling and — because the two photographed launches DISARM that ceiling so the shutter is not racing a 270 ms window — a check that the disarm is an opt-in and stays one, proving the failsafe fires without it, that the window it leaves does not widen when the screen asks for five seconds instead of three, that the curtain is still up two seconds past the instant the identical launch removed it and that the opt-in has exactly the two call sites this file's own source says it has; the switch found behind the gear, turned off and still off after a relaunch, joined to `config.rs`'s durable default and the three registered commands; and the launch cost held in COUNTS rather than in milliseconds — 0 Tauri commands, 0 fetches, 0 XHRs, 0 subresources, 30 DOM nodes with all 30 of them inside `#splash` and none in the shell, nothing `main.js` can await and at most one layout read per frame and never one after a write — each counter carrying a positive probe in the same run, with the millisecond figure measured cold and warm every run beside that run's own noise floor and gated only by `RICHOS_SPLASH_BUDGET_MS`. Then, for the eleven MATERIAL versions RICH-TODOs row 9 added: a material layer's whole vocabulary read off `splash.js` rather than typed here, with an entry carrying a key outside it DROPPED from the pool; each entry's mat photographed as it ships and again with its material stack emptied and nothing else touched and required to differ over a quarter of the mat AND to differ from v0's mat by as much, because a suede that renders as a flat navy rectangle is v0 with a different name; every number in the relief filter WebKit built joined back to the number in the library it came from, plus the merge order, which is the one thing in it that is not data; the settle asserted to pin the mark's relief rather than flatten it; and eleven side-by-side files, the mat as the SHIPPING renderer draws it beside the mat the study each entry NAMES draws. Writes `shots-splash/`. Every check run RED once — `docs/verification/opening-screen-2026-08-30/mutation-runs.txt` for the first thirteen, `material-reduction.txt` beside it for the five that came with the material (and for the one version, v18, that is deliberately NOT in the library). |
| `appearance.js` | THE TWO LIGHTINGS, THE TYPE KNOB AND WHOSE RAIL THIS IS — CEO rulings §14/§15 and his correction to round 10.1. Dark is the default UNDER A LIGHT OS (a build resolving `system` by default would look right on his machine and hand his ruling to a setting he never made); the choice is durable and config.rs wins any disagreement with the pre-paint mirror; the opening screen is always dark with the CEO's own preference left untouched, carries the settings button anyway, carries NO theme switch and neither opening the menu nor pressing Bust a bug lifts its curtain — because a bug report must start from the screen the bug is on; the button is hit-tested ON TOP on six surfaces and (check 7c) on top, in the accessibility tree and opening its menu over EVERY overlay found from the shipped CSS and markup (`lib/overlays.js`: every rule that covers the window, every element it matches and no `aria-modal` anywhere), the sheets its own menu opens included, the two first-run questions main.js holds Settings back over (setup.js check 20) the only ones left out; the menu is theme -> Text size -> Techy Mode -> Bust a bug, in §15's order; ⌘+/-/0 calls preventDefault and moves the SAME persisted number the Text size row does, both directions observed, as does the Techy row against the rail's own preference; every font-size in the shipped CSS is rem off a scaled root and four sampled nodes move by exactly 1.1x together; `zoomHotkeysEnabled: false` is claimed in tauri.conf.json rather than inherited; the wordmark replaced 'My Company' and re-inks per theme; and the rail footer shows his initials and name or — when there is no name — an EMPTY circle and 'Set your name', never an invented name and never '??'. Also the GUARD-RAIL on the splash off switch: it survived the menu rebuild, is present behind BOTH the gear and the settings button, moves as one state in both directions and the local mirror splash.js reads on the next launch agrees with it. Writes `shots-10-1/` (six surfaces x both themes, plus the always-dark start screen). Every check run RED once — the mutations are listed at the foot of the suite. |
| `techy.js` | TECHY MODE, the renderer and the toggle (`machinery.rs`, `journal.rs`, `machinery_view.rs`) — open-items row 3.1 Phase 2. The turn's real tool calls with the merged command rather than the wire's placeholder title and the status each actually returned; `outcome not recorded` never folded into `done`; no rollup in technical mode, where three commands are three facts; the untyped vendor kind and the auto-approved permission present here and absent from the calm view; the shortcut pinning ONE conversation and the Settings switch reaching all the unpinned ones; a pin handed back to the default, which is what keeps §7.1 open; the calm view asserted BYTE-IDENTICAL across a round trip; the honest empty state for a thread from before the routing commit and — separately, because they are different sentences — a store the OS refuses; the raw pane's three answers; no control of any kind; and no row for `agent_thought_chunk` or `fs/*`, which provably never arrive; and — added for dev-walk audit N3, 2026-09-17 — the badge (`#techy-chip`) and the fixed settings gear never occupy overlapping rectangles at the app's documented minimum window (1024x700), with the longer of the chip's two sentences driven rather than the shorter. Joined to the live Rust payload through `fixtures/machinery-payload.json`. Writes `shots-3-1/`. Every check run RED once — the mutations are listed at the foot of the suite. |
| `techy-race.js` | THE TECHNICAL VIEW SWITCHED OFF WHILE A RELOAD THAT ASKED FOR IT IS IN FLIGHT (review rv-20261021T195352Z-f24b36ae-2380; failure class `docs/development/failure-classes/machinery-in-the-user-view.md`). No browser and no clock: the shipped `techyOn`, `loadTimeline`, `confirmTechyScope` and `renderBetweenTurns` and `timeline.js` in a sandbox, the bridge replies released by hand in the order that went wrong, the committed `fixtures/machinery-payload.json` as the late reply. The late technical reply is discarded and the calm view stands; positive control: the same reply draws the rows while the view stays on. Run RED without `loadTimeline`'s view clause. |
| `composer-scroll.js` | THE COMPOSER'S SCROLLBAR — dev-walk audit N1, 2026-09-17: a white native scrollbar jammed through the message box's rounded corner and its own gold focus ring, on an empty box with nothing to scroll, in both themes. An empty composer's `scrollHeight`/`clientHeight` invariant, in both themes (asserted rather than proven able to fail in this engine — said plainly in the file's own mutation log); a genuinely long composer really does overflow and the textarea itself carries no border/radius of its own any more (moved to a new, never-scrolling `#input-shell` wrapper, which is what actually clips a native scrollbar to the rounded corner — WebKit does not clip one to the SCROLLING element's own radius); and the focus ring lives on that shell rather than on the textarea. Checks 2 and 3 were run RED against the shipped shape and GREEN against the fix, in this session. |
| `retention.js` | THE RAW-RETENTION WINDOW AS A SETTING (`journal.rs`'s `RawRetention`, `config.rs`'s `RetentionChoice`) — techy-mode design §7.2, open-items 1.4. §7.2 is the CEO's question and nothing in the suite answers it; what it proves is that every answer he could give now costs a click rather than a developer. The control found behind the same gear as the technical-view toggle, with three settable choices; those three joined to `RetentionChoice::parse` and to the harness's own window table, both read off disk, so a radio the backend would refuse fails here instead of silently doing nothing; the labels joined to the NUMBERS they stand in for (`RAW_RETENTION_DAYS`, `THREE_MONTHS_DAYS`), which live in a different file from the words; a tightened window announcing what it removed and which half of the record went, because `evict_raw` is an `unlink` and nothing else in the product would ever mention it; the loosened window claiming no removal and no restoration; both axes and the store's current cost in one sentence, so "keep everything" is an informed choice; no `localStorage` mirror for a setting that DELETES, with the store winning over the last click on every open; a hand-edited window reported as itself with no radio rounded on; and a refused choice leaving the surface on the store's answer. Writes `shots-7-2/`. The survival counts at four windows are Rust's half and are in `crates/richos-core/src/journal.rs`. Every check run RED once — the mutations are listed at the foot of the suite. |
| `contrast.js` | THE CONTRAST FLOOR (`CLAUDE.md` §"Contrast — WCAG AA, ALWAYS, BOTH THEMES"), computed rather than eyeballed. Seventeen driven surfaces × two themes: every visible run of text and the bounded non-text-indicator subset, resolved against the colours actually painted behind it via the browser's own paint stack, at 4.5:1 / 3:1. The arithmetic is checked against WebAIM's published values and then proven to be the SAME SOURCE running inside WebKit rather than a second copy. The exemption is machine-readable — `data-contrast-exempt="<why>"` — and a declared one passes AND IS PRINTED with its reason and the ratio it was excused at, while an empty or one-word claim fails as a mute button; check 12 prints the whole inventory so creep is a number. An unresolvable colour (gradient, blend mode, filter, backdrop-filter, transparent text) is a FAILURE TO PROVE and has its own ledger, currently one entry: the opening screen's tagline, which sits on a gradient under two blend-mode layers and cannot be proven by any DOM checker. Text behind an `aria-modal` dialog is inert and is filed obscured and check 11 refuses to let that bucket become a hiding place by requiring every node in it to be measured on a surface where the dialog is closed. `contrast-debt.json` holds the 53 colour pairings the shell was ALREADY failing on 2026-08-30, capped so it can only shrink, plus a per-surface node floor in the spirit of run.js's `observed >= declared`. Writes `shots-contrast/`. Every check run RED once — the mutations are listed at the foot of the suite. |
| `updates.js` | THE UPDATE SURFACE (`src-tauri/src/updates.rs`, `ui/updates.js`) — RICH-TODOs row 12, which said there was no updater of any kind. What a browser CAN prove, with what it cannot said first: it cannot apply an update and that is proven where it happens, by `richos/app/scripts/updater-e2e.sh` building 0.1.0 and 0.1.1 and making one become the other on this machine. Here: the row lives in the UNIVERSAL settings menu and is reachable from every screen; all nine states `updates.rs` declares render a sentence, with the inventory read out of the Rust rather than typed and an UNRECOGNIZED state reporting itself as unrecognized instead of falling back to "up to date"; "never checked" and "checked and current" are different sentences; the shipped `.invalid` endpoint reports a DECISION NOT MADE rather than a failure and a non-default endpoint is disclosed on screen; a REFUSED SIGNATURE is not offered a retry while all six other failure kinds are; the vendor's own error text verbatim behind a disclosure; no percentage and no `aria-valuenow` without a `Content-Length`; the mark on the settings button for exactly two of the nine states, arriving with the menu shut; Install and Restart asserted on the COMMANDS ISSUED rather than on a button looking pressed; and the row surviving a `forceDark` menu rebuild. Then THE CUE (CEO ruling §26's placement paragraph, 2026-09-04 — *"the user can't be bothered to hunt for some update button somewhere"*): a waiting update raises exactly one small element in the chrome that is on every screen, with NOTHING opened to see it and the version named in it; the eight states that are not waiting on him raise NO element — asserted by count, by driving the state back after the element has once existed and by the settings wrapper measuring exactly its own button when there is nothing to say, which is the assertion a hidden placeholder cannot pass; the settings button's right edge proven not to move when the cue arrives; and the cue leading to the row it announces — the same sentence, focus landing on the row's own control, `update_install` issued by THAT control and by nothing the cue did itself. Contrast for this surface is deliberately NOT here — it is `contrast.js`'s three `updates-*` surfaces, which walk the cue's label in both themes. Writes `shots-updates/`. Every check run RED once — the mutations, the two that reddened more than their own check and the one that exposed a hole in this suite's own helper, are listed at the foot of the suite. |
| `setup.js` | FIRST-RUN SETUP at the surface — Option D (`crates/richos-core/src/setup.rs`, `src-tauri/src/setup_view.rs`). The launch blocker `ceo-decisions.md` §19 states in its own words: *today RichOS runs on his Mac and would not run on anyone else's*, because a customer needs Claude Code AND the engine directory and the engine ships in no payload. What this suite holds: a customer's Mac ASKS and asks this before the memory question and before the company question — with both held-back questions proven to be asked rather than dropped; NO terminal, no path, no tilde, no shell variable and no version number anywhere on the sheet, computed from the rendered text rather than intended and ZERO text fields, because his part is one press; the BYO-Anthropic caveat present and ABOVE the button, compared by document position, because row 3.14's second condition is that D must not be sold as zero-touch; each missing piece named AND explained, with the title agreeing with the count; a build that cannot install an engine EXPLAINING and naming the party instead of drawing a button that would certainly fail; a failure rendered verbatim with the sheet still usable and the button relabelled to say what pressing it does now; PROGRESS driven by the backend's events rather than by the return value, asserted through a MutationObserver over the whole run, because a sheet that only rendered the answer would sit silent for the minutes Anthropic's installer takes; a machine that has everything neither asked nor skipped-without-checking; the button un-pressable twice; and — structurally, over the shipped source — exactly one `run_setup` call site. AND, since candidate .15 (`esc-20260919T152225Z-2e44d112`), that the offer reaches the SCREEN and not only the DOM: case 18 is the only check in this directory that does NOT call `leaveHome()`, because `leaveHome()` is `RichHome.hide()` through an API a person does not have. Ray arrived on the home screen and measured under WebKit that screen is `z-index: 150` over `.overlay`'s `60`, marks `#app` `inert` (which is where `#setup-sheet` lives) and pulls focus back to its own door — so `init()` opened the engine offer into a place he could not see, press or focus and one Escape pressed its "Not now" for him without his ever having seen the question. Four live captures, no sheet and every message he typed died with "I lost my connection to the part of me that thinks". The check opens the app the way he does and asserts the offer is painted at the middle of its own panel, holds focus and that Escape presses its NAMED way out. Since candidate .16 (`esc-20260919T171553Z-e42d1166`) it also boots the way the app boots, because it had gone green over a defect that was still there: `AXFocusedUIElement` read `text area Message to Rich` with the offer on screen. `mock.js`'s bridge costs nothing, so `init()`'s await chain finished BEFORE the parser did and `home.js`'s give-way — installed at `DOMContentLoaded` — ran last and won; with the bridge costing one task per command, which is what a Tauri IPC round trip is, the give-way lands focus on `#setup-go` at t=252 ms and `init()`'s tail takes it to `#input` at t=304 ms. The case now puts that cost on the bridge by intercepting the assignment `mock.js` makes and waits for the CURTAIN to leave — the product's own word that `init()` is past its last focus call — rather than for the give-way, which fires 52 ms too early. Its Escape half could not fail at all before: it asserted `!homeOpen || !sheetHidden` three lines under a wait for `#home` to be hidden, so `homeOpen` was false by construction. It now measures the CLICK on `#setup-later`, because a sheet that went away for some other reason is not the same event and only one of the two is consent. Case 19 is the same defect one surface higher — the opening curtain is `z-index: 200` over the same sheet and at `dcebed09` one Escape pressed there took the curtain down AND pressed "Not now" on a question that had never been on screen, which is the key a person reaches for first on the first build he can get STUCK behind a curtain (§62's space-bar hold). Escape now takes the curtain and nothing else; the space bar is the control in the other direction and must reach neither the sheet nor the composer. Contrast for this surface is `contrast.js`'s two `setup-*` surfaces. Every check run RED once — the mutations are listed at the foot of the suite. |
| `memory.js` | FIRST-RUN PROVISIONING at the surface (`provision.rs`, `src-tauri/src/memory.rs`) — the gap the installed bundle was measurably in on 2026-09-01, when its company memory reached it only because an engineer typed a symlink by hand. A fresh install ASKS and asks this before the company question, one dialog at a time — with the held-back question proven to be asked rather than dropped; the location is SHOWN and the string sent to `provision_memory` is compared byte for byte against the string on screen, driven from a location the surface could not have guessed so a hard-coded path fails instead of agreeing; ZERO text fields in the dialog, because his part is a choice and never a path he types; one press producing exactly one command; a refusal from the backend rendered as it stands with the control still live, because `provision`'s messages each name the thing to do; a corpus the install cannot read naming the party and drawing NO button for a thing no button could do; an install that is already set up neither interrupted nor provisioned; and — structurally, over the shipped source — exactly one `provision_memory` call site, passing the offered location, with no corpus path literal anywhere in `main.js`. Contrast for this surface is `contrast.js`'s two `memory-*` surfaces. Every check run RED once — the mutations are listed at the foot of the suite. |
| `escape.js` | ESCAPE CLOSES EVERY POPUP — CEO, 2026-09-17, item 1 of his v1.0.2 list: *"The user must always be able to close any popup of any kind by simply tapping the escape key on the keyboard i.e. without having to click anything"*. Before it, `main.js` handled Escape for eight named surfaces and the window shipped thirteen: the gear's own preferences popover was not among them and `#setup-sheet`, `#memory-setup`, `#set-menu`, `#permission-sheet`, `#repositories-sheet` and `#home-prefs` were not either — three of those carried an Escape listener bound to the popup ELEMENT, so each worked only while focus happened to be inside it. THIS SUITE CARRIES NO LIST: every check derives its surfaces from `window.RichDismiss.selector`, the same query the shipped handler enumerates with, so a popup written next month is under these checks the day it is written. What it holds: every element that is a popup BY STRUCTURE declares how it is dismissed, with a planted undeclared popup as the positive control that the declaration check can go red (and that the handler's blunt fallback still closes it, because the rule has no "unless somebody forgot" in it); every declared surface answers Escape with focus OUTSIDE it and a `control:` surface is asserted to have PRESSED its named button rather than merely to have vanished; the gear popover and the settings menu by their real entrances, with `aria-expanded` following them down; TOPMOST FIRST measured against the computed `z-index` read from the stylesheet rather than a chosen order, one surface per press; a permission request DECLINED by Escape from a hand nowhere near the panel, asserted on the answer and not on the sheet disappearing; the home screen's dialog closing scrim and all; that Escape at the HOME SCREEN cannot answer a surface painted BEHIND it (B7) — the third instance of the defect `splash.js` names, since `isOnScreen()` knows nothing about what is painted over what, with a planted surface above the screen and the real `#set-menu` as the negative controls and the guard proven to leave with the screen; and Escape with nothing open moving nothing and stealing no key. The setup sheet's own half is `setup.js` case 14, which holds the other side: Escape is the named button and mid-install — when there is no named button on screen — it is inert. Every check run RED once; the mutations are listed at the foot of the suite. |
| `front-door.js` | THE FRONT DOOR, MEASURED WHERE THE OTHER SUITES COULD NOT SEE (audit-9 rows 1 and 3). Two rows Ray reported open on candidate .9 after every suite here was green: the faded loading layer stayed in the accessibility tree once the picture was up and a Dock restore (a second window) landed on the home screen because `home.js` asserted that it should. This file reads an ARIA tree (`locator.ariaSnapshot()`, the nearest thing Playwright 1.61 ships — an `opacity: 0` subtree is IN, a `[hidden]` subtree is OUT, measured) rather than the DOM and reads the launch kind rather than assuming one. The native half — a real Escape through AppKit into a real WKWebView — is not here and cannot be, because this harness injects keys through the automation protocol, a path the shipped window does not have; that half is `scripts/front-door.test.sh`. |
| `onboarding.js` | THE FIRST-RUN NOTICE — the visible half of the onboarding offer (`crates/richos-core/src/onboarding.rs`, `src-tauri/src/main.rs`'s `onboarding_view` / `decline_onboarding`, `docs/plans/richos-first-run-notice-2026-09-06.md`). The gap it closes is named in `docs/verification/onboarding-honesty-2026-09-06/`: the interview is offered INSIDE a reply, so a CEO who types nothing or who does not read the first reply, is never asked at all. What this suite holds: the offer is on screen with NOTHING typed and NOTHING sent — asserted through the recorded invokes, not inferred — and the three states that must render nothing render nothing, which is the negative control that makes the first assertion mean anything; "Not now" issues exactly one `decline_onboarding` and the BACKEND is then asked whether it agrees, because `record_declination` shipped with no caller at all and "the panel closed" is precisely the evidence that would have passed over that; a REFUSED write keeps the offer live, re-arms its control, names what did not happen and puts no path on his screen; Start sends one ordinary `send_message` that lands in the record as his own turn, with the shipped window proven to name the skill nowhere, so there is no private route to drift from `OFFER_BLOCK`; the unusable state draws NO control and never offers an interview over notes it cannot read; no sentence anywhere on the surface implies staffing or that his own data will appear on the home screen — six forbidden words each, with the one promise it CAN keep asserted present so the check cannot pass by the copy being empty; the mock's copies of three `main.rs` sentences compared against the Rust; and every line's contrast and type computed IN THE PAGE in both themes against 4.5:1 / 3:1 and the 16px floor, with zero `data-contrast-exempt` permitted anywhere on it. Contrast for this surface is also `contrast.js`'s two `first-run-*` surfaces and its states are classified and driven in `affordances.js`. Every check run RED once — nine mutations, with the check each actually turned named, are listed at the foot of the suite. |
| `home.js` | THE HOME SCREEN — `round-11.1/v1` "Constellation" ported into the app, the switch out of it and the way back (CEO, 2026-09-01: it "must be shown in the app after the splash screen", with "some way for the user to switch" and "a click on the logo (in the upper left corner) brings the user back"). Through the real shell: it is the surface the app lands on, over an inert `#app`, at a z-index under the curtain and under the settings button; §15's permanent always-dark exception held as a FORCE flag with the CEO's own light-mode preference intact underneath it and the settings menu on it carrying Bust a bug and no theme switch; the picture asserted to be the round's own numbers — 7,500 objects, 12,817 links, 4,800 sources and v5's `nodeScale` and `clickZoom` — so a tuning pass on a signed-off design fails here; the company row proven to be the REGISTRY's six in registry order with `richos` as ONE button despite two roots, "All companies" as a pressed default rather than an absence, wrapping into rows inside a 500px cap that cannot reach either text column, ABSENT below two visible companies with the composition back at the round's own pixel and a one-character label rendering as a 46px pill rather than a cramped lozenge; the reveal read IN THE PILL it happens in, monotonic at zero tolerance, with the pill's `width` and the track's `transform` required to be ONE declaration (the viewport figure is measured and printed and gated only by `RICHOS_SLIDE_ABS_PX`, because two separately-clocked transitions inside a centered row subtract); a label proven to be a MASK — every entity id unchanged through an anonymizing pass and back; the settings panel listing every company including the hidden ones, writing through to the row live and measured in BOTH themes; the switch stopping the frame loop (frames stop and are still stopped 1.5s later) and the logo resuming it without a rebuild; and CONTRAST measured FROM THE PIXELS, because almost every line of this surface sits over a `<canvas>` that `contrast.js`'s DOM walk states it cannot read — glyph line boxes, border rings with the rounded corners excluded, indicator halos stepped past and the element's own opacity folded into its ink. 16 elements, worst 4.23:1. Writes `shots-home/`, six of which are committed. The give-way half — a desk sheet opened over this screen brings the desk with it (audit-7 row 5) — is here too and since candidate .15 its derivation is "an `.overlay` that is not part of this screen" rather than `body > .overlay`: the old selector covered four of the document's eleven overlays and none of the three questions `init()` asks a customer, which is how the engine offer came to open behind the picture. `setup.js` case 18 is the check that holds it from the other side. Since candidate .16 this file also consumes Escape while it holds the keyboard, unless what is open is PAINTED over it — a floor under that give-way rather than a fix for a live path, since no product path today reaches home-up-with-a-sheet-behind-it. The test is `elementFromPoint` and not a z-index comparison, because `#set-menu` carries `z-index: auto` and is painted at 300 by the `.settings` wrapper, so the obvious derivation read the most-used menu in the product as behind the picture and ate the Escape that closes it (`affordances.js` PART 6 and `escape.js` B7). |
| `voice-model.js` | GETTING THE SPEECH MODEL at the surface (`crates/richos-voice/src/provision.rs`, `src-tauri/src/voice_provision.rs`) — the gap `.github/README.md` named in its own words on the first public release: *"Voice does not work yet … Speech needs a model this build does not download for you."* The rules are covered without a browser (23 Rust tests, no socket opened) and the transfer is covered without a window (`src-tauri/examples/provision_model.rs`, a real 487,614,201-byte download verified against its pinned sha256); what only a browser can hold is here. **THE INVARIANT FIRST, above every control it checks: the offer path opens NO microphone.** Published v1.0.0 shipped a talk button that asked for the mic, said "listening…", lit macOS's orange indicator and never transcribed (ray-opus-a1, 2026-09-04); the fix was to stop offering the button and this work puts it back on a machine where it now leads somewhere — which reopens exactly that hole, so `start_voice_capture` is asserted absent from the recorded command list on the way to the offer, with the READY path's mic request as the positive control that the assertion can fail. Then: a decoder-less machine is offered nothing, because a download cannot help it; the offer names its size from `model-costs.json` rather than from a literal; the bar is driven by measured bytes (121,903,550 of 487,614,201 renders as 25% and a 25% bar) rather than by a timer; `verifying` gets its own sentence and its own Stop state, because a bar frozen at 100% under the download's words is where somebody decides the app has hung; Stop is asserted on the COMMAND ISSUED and the row is required NOT to flip until the backend says so; a captive portal renders with the retry control its own sentence tells him to press and pressing it is proven to be a second real attempt rather than a re-render; a refusal asking again cannot fix draws no control and names the party; installing does NOT open a microphone by itself and Start listening does; and a download the panel was closed for is not lost. Exactly one row on screen at a time, asserted as a set rather than four `hidden` flags. Contrast for this surface is `contrast.js`'s three `voice-model-*` surfaces — the first walks of `#voice-panel` in this suite's history, which its own header had listed as unreachable. |
| `slow-bridge.js` | CODE THAT IS ONLY CORRECT WHEN THE BRIDGE IS INSTANT — the two shipped defects `tom-opus-bw1` measured on 2026-09-06 (`esc-20260906T085912Z-4005a6b5`), each held at THREE latencies: 0, 120 (an ordinary machine and where both were measured) and 400 (slower still). A thread pressed during boot is the thread that opens — three presses per latency across the window `init()` used to own, at 3, 3.5 and 4 call-times after the row appears, because pinning one number measured on one Mac is how a negative check passes for the wrong reason; the shell REMEMBERS the thread he pressed, since a `switch_thread` race is what the NEXT launch restores from; and, as the half a fix written as "never restore" would break, a boot with no press at all still lands where the last session ended. Then the update cue: the press and the read of `document.activeElement` happen in ONE synchronous block — no task turn, no frame, no round trip — so a deferred focus cannot pass it at any latency including 0, Enter is required to reach `update_install` and the control is sampled every frame across a 400 ms `update_state` read and required to be pressable on all of them. And, added 2026-09-19 for Ray's candidate-.11 §2.2, the send that CREATES a thread: every animation frame across `create_thread_in` → `openThread` → `send_message` is sampled and the first-run greeting — “I'm Rich — your chief of staff…” — must appear on none of them, his sentence must never leave the screen once it has arrived and it must be drawn exactly once. Opens a browser; writes no shots. Every check run RED once, against `main.js` and `updates.js` restored to `041eee8`: 6 of the 9 went red naming the wrong screen and the dead control and the 3 that stayed green are the control cases — the list is at the head of the suite. The candidate-.11 check was proven the same way against `main.js` restored to `05ab7a0c`: 8 of 223 frames at 120 ms and 24 of 391 at 400 ms held the greeting, with 0 ms green as its control case. |
| `markdown.js` | §5.4 MARKDOWN IN RICH'S ANSWERS, which this surface never rendered — on the published v1.0.1 the first answer a customer ever received read `**1. Tell me about Lakeside Advisory.**` on screen, literal asterisks and all, because the engine emits Markdown and `renderRichMessage` set `textContent`. The subset `timeline.js` now builds and nothing beyond it: bold, italic, code spans, ordered and unordered lists, headings, paragraph breaks. Three things proven: the subset renders as real elements with the markers gone; an unmatched marker degrades to its own characters and NEVER eats the tail of a message (six malformed inputs, each asserted character-for-character); and model output never becomes markup — `<img src=x onerror=alert(1)>` and a `<script>` tag driven through the real renderer with the characters asserted ON SCREEN, the elements asserted absent, the angle brackets asserted escaped in the DOM's own serialization and the same repeated INSIDE a bold span, a code span and a heading. There is no escaping step to forget because there is no HTML-string sink at all, which is checked over the stripped source. Both write sites are covered and asserted to produce byte-identical markup, so an answer cannot change shape at the moment it completes. Contrast is checked here rather than deferred: every Markdown node is asserted to inherit `.tl-prose`'s `--ink` — no new color is introduced — at the §17.2 18px scale, with a code span distinguished by `--mono` alone. |
| `outage.js` | THE FAILURE CARD WHEN THE MODEL API IS WHAT FAILED (`crates/richos-core/src/upstream.rs`) — open-items row 3.30, measured on 2026-09-03 when `529 Overloaded` killed four running agents mid-task. Until this suite the card said two fixed sentences for every failure and the second of them — *Everything I'd already written above is saved* — is true about the text and false about the whole: the session's working context is gone and those five agents died having written nothing at all. Here: a `529` REPLACES both generic sentences with the backend's own three (what happened, what is on disk and what is not, what was spent trying), with the generic note asserted GONE rather than merely outnumbered; the retry control still present, enabled and carrying the same verb; the attempts line absent on the first failure and present on the second, because *nothing spent yet* and *nothing was spent* are different statements; and a `429` rendering a DIFFERENT sentence from a `529`, which is the whole of the row's fifth answer. Two negative controls: a failure with no outage still gets the generic card unchanged and a completed turn grows no card at all. Nothing here types a product sentence — every one is scraped out of the Rust at run time by `lib/state-strings.js`, so reworded copy moves this suite with it instead of leaving it green over words the product no longer says. Contrast is computed from the pixels WebKit actually painted, in BOTH themes, with the 4.5:1/3:1 floor chosen from the MEASURED font size: 12.06:1 and 5.52:1 dark, 18.07:1 and 5.35:1 light, every line 16px. |
| `interruption.js` | THE FAILURE CARD WHEN THE FAILURE IS LOCAL (`crates/richos-core/src/interruption.rs`) — the 2026-09-17 nightly's D2. The published build held `cognition protocol: "Not logged in · Please run /login"` in its own conversation ledger and showed him *I hit a snag mid-thought*, *Everything I'd already written above is saved* and a **Pick it back up** button: a permanent condition described as a passing hiccup, a promise about work that had never been written and a control no number of presses could make work. Each untruth is made unreachable here against the real renderer: the body is the backend's authored sentence with the generic one asserted GONE; the account and the route that exists are named (and the fixture is checked to have **zero** prose rows above the card, so "no saved-answer claim" means something); and the card draws **no** retry control at all, not a disabled one carrying the same promise. Two controls keep it honest: a TRANSIENT failure still draws the button with the same verb and counts the characters that really are on disk and unclassified interrupted/failed turns, with and without partial prose, get a RichOS notice that neither speaks as Rich nor claims unchecked saved work. Nothing here types a product sentence; each is scraped out of the Rust at run time by `lib/state-strings.js`. Contrast is computed by the shared probe from the pixels WebKit painted, in BOTH themes, 0 failures over 6 measured nodes each, every line 16px. |
| `job-question.js` | A BACKGROUND JOB'S QUESTION IS NOT A TURN THAT A QUIT CUT OFF (nightly-38 walk, D8). A job's question is scoped to its obligation, not a conversation turn (`native.rs` `prepare_question_scope`), so the snapshot carries it with a `turnId` no ledger turn has and no `work_duration` row. The renderer used to make a turn record for it from the question alone, with the defaults `queued` and not live and drew "Status unavailable" and "This turn was still running the last time RichOS was open" under a job's open question while the app had never closed. Against the real renderer: the question is drawn with no duration row, no quit card and no turn record; a ledger turn really left running still gets the card (the positive control); and the question keeps its place between the turns around it. |
| `question-timer.js` | "I'LL CHECK." / "I'LL INVESTIGATE." — CEO §58 (2026-09-18): the timer beside his reply to a question the front desk handed over reads `checking` and flips to `investigating` at 60 s on its own; the flip is asserted at 59,999 and 60,000 ms, the worst case re-derived at 1 Hz and the timer's contrast is computed in both themes against the 4.5:1 floor. Real renderer under WebKit. |
| `conversation-while-working.js` | NO ROW THAT JUST SAYS "WORKED" (product hunt 2026-10-15, finding 13). On nightly 57 a divider reading only "Worked" stood above every message Rich sent on his own, because a completed turn with no displayable duration was labeled that way. Against the real renderer: two proactive turns (duration unrecorded and under the one-second floor) draw no duration row while a measured turn keeps "Worked for 15s" and a turn with work to disclose but no measured time labels its disclosure "What Rich did". The nameless activity row is the Rust half (`timeline::activity_item`). |
| `docs-claims.js` | Opens no browser. It joins the claims in `richos/app/README.md`, `richos/app/STREAMING.md` and this file to the tree they describe: every Rust test file named (with no test count beside it), this table against the inventory `run.js` discovers and every `rich://` name against the constants the Rust source declares. Nothing in it is typed — both sides of every join are read off disk. |
| `bug-report.js` | Bust a bug (round 21, the CEO's §115): the Settings button starts the exchange with Rich in the conversation on screen rather than a notice, Never mind, the answer comes back as the report card before anything is sent, names and file paths shown as stand-ins with what they replaced only in the tooltip (by pointer and by keyboard), Send with the words the card showed and from the RichOS reporting account, a failed send kept and sent by itself once the connection is back (Rich's line and the notice), GitHub down, a waiting report changed and approved again, Change it in place with the privacy heads-up, a change told to Rich, Cancel, one report at a time and the Rich panel beside Corrections. The second review's cases (rv-20261009T102303Z-1c3dda1d-4c78): a name Rich found private stays left out through every change and the heads-up names it; Send, Change it and Cancel wait while Rich changes the report; Cancel says nothing was sent only once the waiting copy is confirmed gone, says so when it could not be removed and says the report went out when a retry already sending filed it (Change it the same); Rich is given the screen's words and the window's picture is asked for before the exchange is drawn. The third review's cases (rv-20261009T135204Z-9f4d77d4-1892): a name typed over a stand-in is the user's own text and gets the heads-up before sending; a file address and a drive path typed in get it too; the digest of what Rich checked is 16px in both themes. The fourth review's cases (rv-20261009T142223Z-3d74fe4c-3b6d): the heads-up is the core's answer (`bug_report_private_in`), so an address like alice@büro.de is named; leaving the report's conversation gives the voice back to Rich (the shell's hold, `bug_report_voice`, goes off) and coming back gives it to the report again. The review of 692942153 (rv-20261009T162841Z-69294215-70e6): when Claude can't check the report nothing is offered for sending, it waits on this Mac and the card comes once Rich checks it (one card however often the shell says so), a change he can't check adds nothing, Cancel report takes the waiting one off this Mac and one kept from before comes in the Rich panel. Every state in both themes at 1440 by 900: AA contrast and the 16px floor computed, 3:1 for indicators, no renderer errors. |
| `codex-reviews.js` | "Let Codex review your team's work" (round 20.2, the CEO's ruling §114 of 2026-10-09): the row below Technical view between two rules, off on first run; its five states (off, on, not installed, not signed in, on then signed out or removed) with who reviews now, the switch and that state's words word for word in the ⓘ's tooltip, describing both the ⓘ and the switch, at one row height; flipping (saved, the ring pulses, none with reduced motion; the name and Space flip it too); the nudge on the unavailable switch; the tooltip's hover, focus, pin, press-elsewhere and Esc-only-the-tooltip; on then signed out read when Settings opens. Every state in both themes at 1440 by 900 with the tooltip open: AA contrast for the text and 3:1 for the switch, the ⓘ and the rules, computed and the 16px floor. |
| `spend.js` | API-equivalent token spend (round 22.1, the CEO's ruling §118 of 2026-10-10): `spend_view` and `spend_live` registered in `main.rs`, answered by `mock.js` and called by `spend.js`; the Token spend row in quick settings directly under Technical view with this month's figure; its own screen covering the app view and the home screen with Settings on top; the Meter word for word (header, eyebrow, figure, the two sentences, the legend naming each model's versions and efforts, the tokens line, the footer); no live counter (a landed request puts a dot on Refresh, Refresh brings it in); the live line priced in dollars; the four periods from their local starts; Back, Esc and the row as the current page; opened from Home, Enter on Refresh, a period and Back doing what each says with Home still open beneath; every Settings row that opens a sheet, pressed over the screen, closing it and putting that sheet on top with the focus; the counting state before the first read; the sentences and the tokens line two lines each and every legend mark one line, the Rich panel docked or not; the header whole (every control inside the column, none overlapping another or Settings, no label on two lines) and every word of the Meter inside its column, the widest live line's dollar price, the widest effort label and the longest mark name included (12 agents, a Fable request; "high and medium effort, some not recorded"; all seven Opus versions beside a five-figure amount) and no text on the screen but the figure held to one line, at 1440 by 900 and at the app's smallest window, 1024 by 700, each with the Rich panel docked or not, at Text size 100%, 120%, 135% and 150% (the docked 536px column puts the periods on a second row and wraps the live line); nothing under the header or the footer, the whole Meter on screen undocked and scrolling in its own space in the docked 536px column; no two characters of the figure closer than 4px on the rendered pixels (every period and the panel docked in both themes, round 22.1's stress set and round 22's setting as the control that must touch); AA contrast for text and 3:1 for the marks, Refresh's arrow and dot, the chosen period, the working dot, the Back glyph, the wordmark's gold and the row's chevron, computed in both themes and the 16px floor. |
| `dictation.js` | Dictation's Settings row and sheet (round 19, states 5 to 12 and its more lines; dictation plan slice 2): nothing at all while dictation is not there (DICTATION_READY), the row's line for each state, the sheet off with line 2, turning it on in the drawn order (the microphone, then Accessibility, then On with the cursor in Try it here and the composer's line), turning it off, the refusals with Open System Settings on the right pane and back without allowing, the key from the strip, by the window's keys or by the tool's tap with the two refusals, Accuracy and Saved, lines 1 and 4 on the sheet, the off notice, Rich's offer's `RichDictation.turnOn` and Escape. Every state in both themes at 1440 by 900 and 1024 by 700: AA contrast for text, the placeholder and the indicators computed, the type floor, nothing sideways, every control reachable, no m-dash or n-dash. |
| `dialect.js` | Opens no browser. American English on the sign-in path — `provider_auth.rs`'s `AuthState::Cancelled` message and `main.js`'s cancel-failure catch clause, the two strings the nightly QA audit (`docs/verification/2026-09-17-nightly-1.2.0-20260917.1-silent-audit.md` §D1) found still spelled the British way. Each literal is pulled out by the shape of the call that carries it, never by the word itself, so the enum identifier and its serde wire value are left alone. Both checks carry a positive control: the same extractor run against a fixture that plants the British spelling, proving the clean result on the real file is not a scanner looking at nothing. |
| `richscribe.js` | Opens no browser. THE DICTATION FEATURE IS CALLED RICHSCRIBE (the CEO, 2026-10-19). With `dialect.js`'s readers (`lib/state-strings.js`), every string literal in every shipped UI script (including those inside a template's `${...}` holes), every HTML text node, title and readable attribute, every stylesheet `content:` string, the CEO-facing Rust scrape and every capitalized Rust literal off a log line: none names the feature "Dictation" on its own. Identifiers, files, events and wire values joined to it by `-`, `_`, `.`, `/` or `:` stay and so does the verb "dictate". One declared exemption: `main.js` still matches the "Dictation feature" title of a conversation an install made before the rename. Positive controls first, so a red run still proves the readers see the old name; a second check that the new name is in Settings, the sheet, the menu bar menu, the bar, the offer, the tray tooltip, the macOS key-watch reason and the conversation title. |
| `oxford-comma.js` | The Oxford comma in what Claude writes and its switch in Settings (CEO 2026-10-19, round 33). Through the real shell, the live events `main.js` listens to and the browser mock's durable config: off by default (Rich's reply and the question Claude wrote on a card drawn without the comma before and/or/nor, a code span keeping its comma, the stored message untouched); the row under Text size reading "Oxford comma: OFF" with his explainer under it; switching it on showing the comma again at once and reading "Oxford comma: ON"; the choice kept across a relaunch; and the renderer itself filtering nothing, so the app's own lines are drawn as written. |
| `dictation-overlay.js` | The dictation tool's own windows (dictation plan slice 3): `dictation-bar.html` (the bar and with `?role=flight` the words' flight) and `dictation-menu.html` (the menu bar menu), each loaded in WebKit with a stand-in for the Tauri bridge that records every call and speaks for the tool. Every bar state and problem line word for word against round 19 and Iris's two lines, Fix it only on the microphone and Accessibility bars; the window size, orb and Fix it box the page reports, measured from layout so a mid-rise frame cannot shrink the window; the meter; the flight and its reduced-motion form; the menu word for word with every row, Escape and the arrows reaching the tool. Contrast computed in both themes, the bar's translucent ground composited over white AND black, plus the light drawn over another app's words (TextEdit, Notes, RichOS's own message box) in both RichOS themes and both macOS appearances; every line 16px or more. |
| `vouch-template.js` | Opens no browser. THE MESSAGE THE PULL-REQUEST GATE SENDS A STRANGER, against the file that claims to record it. The live text is a heredoc in `.github/workflows/vouch-pr.yml`; `docs/verification/pr-trust-gate-2026-09-05/raw/close-comment-rendered.md` is a photograph of it and on 2026-09-05 the CEO rewrote the message by editing the photograph — which changes nothing a contributor ever sees. Nothing checked that the two agreed, in either direction. Here: the block scalar is dedented as the runner dedents it, proven by handing the step to a real `bash` with a real `RUNNER_TEMP` and reading back the file the shell wrote and proven again against a real YAML parser, with the naive read asserted to DIFFER so the ten-space trap is shown rather than assumed; the message is rendered through vouch's OWN `template render` at the commit the workflow pins — fetched as a source tarball whose sha256 the evidence records, never re-implemented, because a lookalike `format pattern` is a check that stays green while the job errors — and compared with the recorded block byte for byte, trailing spaces included, because twelve of its line breaks ARE trailing spaces; the brace invariant is asserted SEPARATELY, since both files could carry the same stray `{` and agree perfectly while the render errors and the gate closes nothing; the pinned commit is joined across the workflow and both raw records, so moving the pin without re-rendering fails here rather than in front of a stranger; and the walkthrough's quoted "correct result" opener is joined to the real one — it was already stale when this suite was written, still quoting the pre-rewrite wording. Nothing is typed: placeholder names come from vouch's own `gh-check-pr`, sample values and digests from the evidence's own prose. Needs `nu` and, on the first run, network; neither is allowed to become a skip. Every check run RED once — the mutations are listed at the foot of the suite. |
| `waiting-state.js` | THE WAITING STATE, after the first outside user of RichOS called a long turn "a crashed application" (2026-09-06): *"a lot of spinning wheels waiting … no interaction of feedback"*. Not that the band renders — the four ways it could quietly start lying. It cannot FABRICATE progress: every sentence it draws is matched against a vocabulary nothing on the wire can support (percentages, `step 3 of 7`, `3/7`, almost/nearly done, estimated, remaining, thinking, progress) and the backend's own `summary` is asserted relayed VERBATIM with a real age beside it, while a row with NO summary is timed and never described with an invented one. It GOES STILL when nothing arrives: the mark's computed `animation-iteration-count` is asserted to be 1 and asserted never `infinite`, because a looping mark is a spinner and a spinner spins after a crash. A DEAD turn looks different from a slow one: a terminal `rich://turn-status` removes the band, the duration row states the ending and a later tick does not bring it back. And WCAG AA in BOTH themes, computed from the real DOM with `lib/contrast.js`'s own arithmetic rather than from the stylesheet's tokens — head 14.55/14.90, time 6.47/5.86, detail 6.47/5.86 and 7.88/5.42 quiet, mark 7.68/5.48 and 7.88/5.42 quiet, against floors of 4.5:1 and 3:1, with the painted `data-theme` asserted first so a light run cannot silently be a second dark one. Plus §18's contract: `role=status` with `aria-live=off` and the quiet state announced once per stretch rather than every tick. Drives turns through the callbacks `main.js` registers with `window.RichBridge.listen`, with `send_message` hanging — the shipping contract, since it resolves only when the turn ends — so no renderer function is called directly. `Date.now` is FROZEN inside the page to one controllable value, so the quiet threshold (35 seconds since 2026-09-06) is reached in milliseconds and an elapsed label cannot drift with however long a real wait took. Opens no shot directory; the before/after frames are `docs/verification/waiting-state-2026-09-06/`. The contrast check has been watched to fail, at `#b8a06a` -> 2.04:1. |
| `compaction-notice.js` | THE SECOND CAUSE of the same report: the child pauses mid-turn to compact its own context for 38.1-62.0 seconds (16 measured boundaries) and the calm surface used to be told nothing at all. Neither its timeline nor its words are this file's. The offsets are READ off the committed arrival stamps of a real `claude` 2.1.263 stream (`docs/verification/compaction-notice-2026-09-06/raw/cellT1.jsonl` joined to its `.timed.jsonl` sidecar by position), so the announcement lands 13ms into the turn, the heartbeat 30.000s later and the ending at 43.6s because that is when they really arrived; the three sentences are read out of `richos/app/crates/richos-core/src/timeline.rs` at run time, so a reword in Rust either follows or turns this red. Checks: the pause is NAMED while it happens, from the frame that announces it (`system/status: "compacting"`) and never from the length of a silence; the wire's own 30-second heartbeat does not read as silence (the regression guard on `QUIET_AFTER_MS`, raised 25000 -> 35000 the same day); the longest compaction ever measured (62.029s) is described for all of it; a child that DIES mid-compaction stops being described at 40s and its band is removed outright by a terminal status, so a dead turn cannot inherit a busy label; and no sentence carries a percentage, an estimate or a countdown. WCAG AA computed in BOTH themes on every element the copy lands in — `.wait-detail` 6.47/5.86, `.tl-activity-text` 6.47/5.86, `.tl-activity-state` 6.14/4.98, `.tl-activity-mark` 6.14/4.98, floors 4.5:1 and 3:1. Writes PNGs only when `RICHOS_COMPACTION_FRAMES` is set; the committed frames are `docs/verification/compaction-notice-2026-09-06/frames/`. The contrast check has been watched to fail, at `#a7a7a5` -> 1.94:1. |
| `compaction-progress.js` | THE PACED BAR over that same pause — the CEO's own design, given after he was told a bar would have to fabricate progress because the duration is unknown: *"It doesn't need to know. It just needs to move to almost full with the expected minimum time and then stay at 'almost full' until all finished etc."* So the bar climbs over the SHORTEST compaction ever measured and then HOLDS and the hold is the whole honesty of it. Nothing here is typed: the span is re-derived on every run from every `compact_boundary` frame in the committed cells (n=16, min 38138, max 62029, mean 49865.0 ms) and checked against `machinery.rs`'s `COMPACTION_MEASURED_MIN_MS`, so the constant cannot drift from the record it claims to come from and the wire field name is read out of `timeline.rs`. Checks: it climbs, never goes backwards and NEVER reaches full on time passing alone — 38.1s, 45s and 62.029s (the longest boundary ever measured) all read exactly 92.00%, held still for 23.9 seconds, with the compositor's own painted 553.8px of a 602px track compared against what was asked for; only a `compact_result` frame fills it, landing in 180ms; a compaction FASTER than the floor catches up at a fixed speed rather than snapping (27.75% -> full in 506ms) while the 1ms `too_few_groups` attempt is capped at the 180ms floor, because a 644ms sweep over a 1ms event would make a non-wait look like a wait; a child that DIES mid-compaction loses the bar at the same instant it loses the description, with the width asserted back to 0 so nothing survives off-screen; no text, no `aria-valuenow`, no `role="progressbar"`, `aria-hidden` throughout; a row carrying no measured floor gets no bar at all and a new sentence takes the bar away; reduced motion still ADVANCES it and animates nothing. WCAG AA computed from the rendered DOM in BOTH themes on all three boundaries — the border that marks the bar's extent 4.57/3.35, the fill against the track 6.29/4.87, the fill against the paper 7.68/5.48, floor 3:1 for each, with the track's deliberately faint interior (1.22/1.13) printed and declared rather than left for a reviewer to find. Writes PNGs only when `RICHOS_PACE_FRAMES` is set; the committed frames are `docs/verification/compaction-progress-2026-09-06/frames/`. Every check has been watched to fail — the mutations are listed in that record. |

| `thread-switch.js` | Cached and cold thread paints, rapid-navigation fencing, sends bound to their destination and activation-failure recovery through the shipping shell. |
| `waiting-lifecycle.js` | First-client regressions across command acceptance, failure without a terminal event, preserved drafts, activity age, compaction announcements and snapshot restoration. Runs the shipping renderer with controlled bridge responses and event timestamps. Native IPC and cancellation are covered separately. |
| `onboarding-lifecycle.js` | First-client regressions for preserving drafts on Start, resuming saved partial answers, binding Not now to its displayed company and ignoring delayed reads or receipts after the user changes company. Both themes exercise the resume offer. |

<!-- THE CANDIDATE-.2 ON-SCREEN WALK, 2026-09-17 — five suites, one per defect its §4 records.
     docs/verification/2026-09-17-nightly-1.2.0-20260917.2-onscreen-audit-2.md -->

| `home-fit.js` | THE HOME SCREEN AT 1024x700, the app's own `min_inner_size` and the size the walk's window restored itself to — candidate-.2 defect #4, which rendered "INFRASTRUCTURE" as "INFRASTRUCTUR". Nothing clipped it at the canvas edge: over a 12x11 cursor sweep at that size not one label left the viewport and `clearQuiet()` erased it, because the draw gate is sampled at a label's MIDDLE and the erase is applied to its BOX. Reads the picture's own exported geometry — `domLabelRects`, `nodeLabelRects`, `eraseRects` straight off `quietRects` and the whole no-caption list — rather than re-deriving the quiet-rectangle arithmetic, with check 1 as the negative control ON THAT EXPORT: seven erase rects, each covering the chrome box it claims to and the first-run banner present in the blocker list. Then, over the sweep: no label inside a rectangle the picture rubs out, none under the opaque banner, no chip across a domain name, none outside the window — 453 / 15 / 104 / 0 against the unfixed renderer. Box overlaps and INK overlaps are counted separately, because an overlap under the label's own 8px margin eats no glyph: that distinction is what shows 1400x880 was clean to the eye and not to the renderer. The fit is asserted to be bought with labels MOVED and not deleted (7.28 domain names per cursor position at 1024x700 against 11.39 at 1400x880) and 1400x880 is walked as the control. |
| `home-team.js` | THE HOME SCREEN OF A USER WHO HAS A TEAM — product hunt 2026-10-15, finding 7: after a relaunch the screen showed a made-up company (18 AI specialists, 1,596 tasks, 2,365 h saved, 7,508 memories, six named specialists "working now") to a user who had just hired two. With `home_team` answering two teammates, it holds that the screen shows "2 AI specialists" and "Your team" with each name and role, that no number or name of the demonstration is on it, that neither the demonstration's dataset nor a field script is loaded, that the banner calling the screen a demonstration is down and "waking loro…" is gone; that the text clears WCAG AA measured from the pixels behind it with the operating system seeded dark and light (the screen is clamped dark); that his own loro, when drawn, does not put the dataset's lines back over his team; and, as the negative control, that a user with no team still gets the first-run demonstration (CEO 2026-09-06). |
| `first-run-sheet.js` | THE COMPANY SHEET'S DEFERRAL, at both sizes — candidate-.2 defect #5. "Sits with the control it explains" as two numbers rather than as taste: the gap above the sentence and the gap below it and it belongs to whichever is smaller. Before: -4px above (it OVERLAPPED "Add this company") and +12px below, to the "Not now" it is about. And the worse defect found while measuring the first, which the walk could not have seen because the CEO's own first launch has an empty registry: with six companies the panel is 865px of content, `.overlay`'s `padding-top: 12vh` starts it at 108px, `.overlay-panel` was `overflow: hidden` with no clamp and the "Not now" was not below the fold but UNREACHABLE — Playwright refused to click it sixty times over thirty seconds. The suite presses the button rather than only measuring it and re-asserts D5's own invariant afterwards: deferring leaves the composer blocked with "Choose the company" beside it, never an armed composer over a company nobody chose. |
| `composer-off.js` | THE COMPOSER WHEN SEND IS OFF — candidate-.2 defect #6, where the box read "Talk to Rich…" over a field that would take his sentence and refuse to send it. The measured pre-fix state is recorded in the file and is not quite the audit's description: the keystrokes were not discarded on the way in, they were accepted, held and refused on the way out with nothing about the box changing. `showUnboundView` had already written the rule down for the OTHER blocked state — "an inviting 'Talk to Rich…' above a dead field is the composer telling a small lie about what it will do" — and the company block never picked it up. Holds: the placeholder says send is off, the box and the send control are both switched off, nothing gets in and the reason is ATTACHED to the control by `aria-describedby` rather than merely sitting above it. Carries the audit's own control (choose a company and the same box types immediately) and a negative control (an ordinary launch comes up live), plus the switched-off placeholder's ratio computed on the rendered frame in both themes — 5.52:1 dark, 5.35:1 light — by `lib/contrast.js`'s own method narrowed to one node, because a placeholder has no line boxes and the DOM walk cannot see it. |
| `control-names.js` | THE COMPOSER'S CONTROLS, NAMED — candidate-.2 defect #7, where the send control's accessible name was the glyph "▷" and "Send" lived only in `AXHelp`. Computes the name by the accname path that applies to these elements and computes `title` SEPARATELY, never counting it, because a check that folded the tooltip into the name would have called the broken state fine; the file declares plainly that this is an approximation of one platform's tree, since Playwright 1.61 ships no accessibility snapshot API. Holds: every visible control in `#composer-row` has a name with a word in it, no control leaves its real name in its tooltip, the glyph is `aria-hidden` so it cannot creep back into the computation and the composer's own name HOLDS STILL while its state changes — it used to fall back to its placeholder, which is state, so the control was renamed underneath a screen-reader user whenever send was blocked. `#talk-toggle`, which the audit named as correct, is the control. |
| `rich-model.js` | RICH'S MODEL IN PREFERENCES (CEO §119, round 23). Two rows behind the rail gear, first in Preferences and absent from the quick settings, each Opus at high effort or Fable, with Stu never named. The rows show the stored choice; a row changed while Preferences is open sends no command, writes nothing and shows no toast (by click and by arrow key); closing Preferences by the gear, Escape or a click outside saves both rows in one command and says what is now true in one toast and a choice flipped and flipped back saves and says nothing; reopening and relaunching show the saved choice; and a reopen while the close's save is still on its way (held by a gate, not a clock) never paints the old choice back. The lease moving to it at its next boundary is Rust's half (`tests/rotation_tests.rs`, `src/work_host.rs`). Red on `main` 4/4. |
| `rail-initials-fit.js` | THE RAIL'S INITIALS FIT THEIR GOLD CIRCLE — walk 40, D12. At the 14px floor the 26px disc was as wide as "MW" and the W's top corner stuck out 2.32px past the edge, onto the sidebar ground. Renders MW, WM, MM and WW in both themes and measures each glyph's ink (canvas `actualBoundingBox*`, placed by the per-character Range rect) against the circle with a 1px margin. Fails on the 26px disc, passes at 34px (1.26px clear in the tightest case, WW). |
| `new-chat-or-company.js` | "+ NEW CHAT OR COMPANY", AND EVERY COMPANY A SECTION HEADING — design round 32, built on the CEO's "yes" of 2026-10-19 to `design/mockups/rounds/round-32/new-chat-or-company.html`. Every line of the round's "What changed", driven through the real window with the bridge narrowed to the one or two companies the round draws: the button's words and its ⌘N; the menu right under it ("New chat in", each company, "New company"); one click starting a new chat with its "New chat" row and gold dot at the top of the company's section; ⌘N with one company starting the chat at once and with two opening the menu on the first, arrows, Enter and Escape; New company turning the menu into the app's own `#entity-add`, the app's refusal on a blank name cleared by typing, Enter adding and the company arriving with its light and a new chat open; the heading itself (folder first, the app's gold, 16px in full ink, chats indented under a guide line, a closed folder when collapsed); and the plus's "+ New chat" tooltip, read in the same task as the pointer's own event with no transition or animation, beside the plus and on keyboard focus. Photographs the round's seven states in both themes into `shots-32/`. |
| `settings-fit.js` | THE SETTINGS PANEL AT 1024x700, the app's own minimum and the size it restores itself to — candidate-.4 defect 3, where the panel measured 738pt in a 700pt window, did not scroll and put **"Bust a bug!"** below the edge. CEO ruling §15 puts that button on every screen and a button below the fold is not on the screen. THE WINDOW SIZE IS THE SUBJECT: `appearance.js` opens every page at 1400x950 and `ticker-wrap.js` at 1400x880, so neither could ever see it — three of that walk's defects are the same shape, something measured at a comfortable size and shipped to a smaller one. The panel's height depends on state (9 rows are 523px; Ray's 738px is the same panel with the Updates block opened on an error), so the negative control GROWS it to his measured height and strips the bound, reproducing his geometry rather than describing it: 104px below the edge, the button ending at y=797 in a 700px window. The positive control clamps the same panel to 616px, scrolls 122px and reads the button back inside the window. A fourth check re-runs it at 1024x520 so a `max-height` in px could not pass. |
| `ticker-wrap.js` | THE TEMPORARY LINE'S WRAP — candidate-.2 defect #9, where "→ 1 new memory" split with "memory" alone on the second line and candidate-.4 defect 2, where the fix for it was still not on screen. Asserts the RENDERED FRAME and never the declaration: the repair is one CSS property, a hint and a check reading `getComputedStyle(...).textWrapStyle` would pass on an engine that parsed the value and did nothing with it. **AND IT MEASURES THE FRAME THE OTHER ENGINE WOULD DRAW**, which is what the first version was missing: Playwright ships WebKit 26.5, Tauri renders through the system WebKit (18.6 on the CEO's Mac) and `text-wrap: pretty` is honored by the first and not the second — so this suite was green while two of four lines orphaned on his screen. It now forces each value the property can resolve to, one at a time, with `auto` as a negative control that must still orphan (394/52px and 397/61px, Ray's two lines). The shipped value is `text-wrap: balance`, which both engines have. Line boxes come from `Range.getClientRects()` grouped by top edge and the words on the last line are counted rather than guessed from widths. The strings are not the file's: the audit's own sentence copied out of frame 33, including the `<i>` the engine wraps the domain label in, plus 24 distinct lines `__loro.ingest()` actually wrote, captured and replayed so each can be measured without racing its 2,600ms life. Two of those 24 orphaned against the unfixed stylesheet, so the reported frame was not a one-off. The first check is a negative control: the audit's sentence must still WRAP here or everything under it passes on a sentence that happens to fit. |
| `chrome-align.js` | THE SETTINGS BUTTON AND THE TWO COMPOSER BUTTONS LINE UP — CEO, 2026-09-19: *"the settings button needs to be properly centered vertically and the right padding for that button needs to be reduced to 15px. And the 2 buttons at the bottom need to be either vertically center aligned relative to the text input or have the same height as the text input because otherwise it looks weird."* Measured on `f918f185`, at 1024x700 and 1400x950 alike: the button's center 12px below the header's with six pixels of its box hanging through the header's bottom border, 18px of right padding and the two controls 6px shorter than the field and 3px below its center. Asserts HIS SENTENCE and not this tree's answer to it — a control passes if its center matches the field's OR its height does, so a later redesign that centers them instead is still green. The negative control comes first and reproduces the shipped geometry exactly (12 / 18 / -6 / +3) rather than describing it; the composer is driven through idle, working with stop and send both up, stopping, voice mode, a grown field and 120% text size, each through the shipping path. Red on `f918f185` at 11 of 13, green here at 13. |
| `speckled-ground.js` | THE DESIGN SYSTEM'S SPECKLED GROUND IN THE DESKTOP APP — CEO, 2026-09-29: *"After the design system addition lands, get it integrated in desktop and mobile apps."* Through the real shell in both themes: the ground is on screen (the stage and rail photographed with the field and again after `RichSpeckle.destroy()` and the pixels must differ); `speckle.js` and `speckle.css` are the design system's files byte for byte, by sha256, mounted with the `desktop` preset; the rule holds — never on the opening screen or the home screen and back when the shell returns; and every line of text on a ground plane keeps 4.5:1 (3:1 large) over the strongest point the live engine can paint. |
| `navigation-evidence.js` | THE HARNESS'S OWN NAVIGATION CAPTURE, proven against a local server that never answers, after the unexplained 30-second `page.goto` timeout of 2026-09-21. The capture is installed on every page and silent on success; a load stalled on one image stays RED with Playwright's message unchanged, fails at its own 1500 ms deadline and leaves a bundle showing `domcontentloaded` reached, `load` not and the image in flight; with no timeout passed the page's own default decides; a page whose script never yields cannot hang the collection; `RICHOS_UI_NAV_EVIDENCE=off` removes it; its cost on a passing load is printed against the same page without it (`lib/navigation-bench.js`); and on a host too loaded to start a helper process in time (`fixtures/loaded-host-preload.js` refuses every `spawnSync` with `ETIMEDOUT`, as `top` was refused on 2026-09-29) the bundle still has its CPU sample and the failure stays red. The failing runs happen in `fixtures/navigation-hang-suite.js`, a child process with its own evidence directory and no ledger. |
| `tracked-tree.js` | NO TEST WRITES INTO THE TRACKED TREE IT IS VERIFYING — 2026-09-29, when the merge gate verifying `cc/echo-opus-speckle1` in the main checkout was refused by its own tests: `splash.js` wrote two legitimately changed pictures over their committed references, the gate saw its source change during verification and canceled every check. Proves, through the real `publishShot` and a real committed file, that a changed picture is NOT written over its reference and IS kept in `.shots/changed/`; and, through a child suite against a throwaway repository (so the proof itself never touches a tracked file), that the harness's end-of-suite guard fails a suite that rewrites a tracked file or creates an untracked one, naming the file, while a suite that writes nothing, a file a person had already edited before the run and a write into gitignored scratch all pass. |
| `gate-honesty.js` | THE UI GATES MUST BE ABLE TO FAIL — part-2 hunt, 2026-09-29, sections 25, 26 and 42, three places the test machinery reported green over the defect it exists to catch. A picture that differs from its committed reference now FAILS the suite that took it (the reference is still never rewritten; the failure names the picture and the `RICHOS_SHOTS_REGENERATE` command that accepts it); `run.js`'s coverage floor counts only a suite's own checks, not the housekeeping check the harness appends, so a declared check that never ran cannot be covered for by the tracked-tree guard; and `appearance.js`'s "no page errors anywhere" check reads every page's live error list when it runs, not a copy taken as the page opened (`lib/page-errors.js`). And two the verification reliability PRD named on 2026-10-20: a failing picture leaves its actual, reference, difference and state in the run's own results folder (the corrections failure of 2026-10-19 could not be read because only the checkout kept it) and a run under `RICHOS_SHOTS_REGENERATE` fails naming every picture it wrote, so the run that drew a reference never approves it. No browser: child suites run the real harness and the floor is exercised through `run.js --coverage` over fabricated receipts. |

## Four rules, each one a thing an earlier slice got wrong
**This table is checked, not maintained by memory.** `docs-claims.js` fails if a suite
`run.js` runs has no row here or a row names a suite that no longer exists. It was added
because `steering.js` had shipped one slice earlier with no row — the same drift `run.js`'s
own discovery exists to prevent, one level out in the documentation.

**1. The real renderer, never a copy.** Every page loads `../timeline.js` and `../style.css`
from disk. A test that re-implements a rule proves the test.

**2. WebKit, not Chromium.** Tauri renders through WKWebView on macOS. A green Chromium run
says nothing about what the CEO sees. `workers.js` caught a real WebKit-specific fact this
way: pressing Tab from a focused button lands on `BODY`, because macOS ships "Full Keyboard
Access" off and WebKit honours it. That is a system preference, not a renderer defect — and
it applies to every button in the app, not just the new ones.

**3. Nothing that only runs when somebody remembers.** `affordances.js` derives its own
inventory from disk on every run and refuses to report green over an empty one. The rule it
enforces is the one thing in this directory that has to outlive the pass that wrote it: a
rule with nothing enforcing it is the defect this project has found eleven times in two
days. Its part 5 is deliberately REPORT-ONLY and prints the number that made that call
(precision 70.8%, recall 54.8%, measured over all 112 states) — a check that cries wolf gets
deleted within a day and then the CEO is worse off than before it existed.

**4. No faked screenshots.** `screencapture` on this machine has returned an all-black
1920x1080 PNG for three slices running (display locked). Every screenshot here comes out of
WebKit's own compositor, which does not depend on a display server — and every one is
decoded and pixel-counted before it counts as evidence. A shot with fewer than 8 distinct
colors across the sample grid throws; the real app measures 175. That check was documented
in `lib/harness.js` for one slice before it existed (the function measured file size, which
is exactly what a valid all-black PNG passes) and now it runs.

**4. A check that cannot fail proves nothing.** Every negative here carries a positive probe
in the same run — the same content, correctly scoped, IS on screen — because "the foreign
thread's text did not appear" passes perfectly on a page where nothing appears. Every
derived inventory is asserted non-empty before it is compared, because a green run over an
empty set is how a scanner in this repository reported CLEAN while walking nothing. And
every check in `restart-scope.js` was additionally run RED once, by breaking the thing it
guards in the shipped source: the ten runs are transcribed in
`docs/verification/restart-scope-2026-08-30/mutation-runs.txt`, with a coverage map and the
one check that has no mutation of its own named rather than left to be noticed.

Two of those runs changed the tests rather than confirming them, which is the argument for
doing it at all: deleting the fence's `threadId` clause left the scope check green (it was
probing across ENTITIES, which a different clause catches) and disabling the renderer's
supersession merge left the crash check green (the reload re-projects from a snapshot where
the superseded turn contributes nothing, so the END STATE was right either way).

## What these tests do NOT cover

They exercise the renderer and the shell. They say nothing about whether the backend emits
what the renderer reads — that is
`cargo run --example timeline_payload` in `richos/app/src-tauri`, which prints the wire payload from
a real ledger through the real command body and `cargo test -p richos-core`.

`docs-claims.js` is the one exception and does not use a browser at all: it reads documents
and source files. It checks that the claims are TRUE OF THE TREE, never that the behavior
they describe works — that is what everything else here and the Rust suites, are for.

## What runs this

`.github/workflows/ui-suite-ci.yml`, on `macos-latest`, on a push that touches anything these
suites read. The counting that makes a green run mean something lives in `run.js`, so a
developer typing `npm test` gets the same gate the runner does.

**It runs in six shards and one job adds them up.** It used to be one job running the suites
one at a time: 1792 seconds on run 34435558131, against 24 seconds of setup. Each shard now
runs a packed slice of the same disk-discovered inventory and writes a **receipt** per suite
— the suite, its exit code, its ledger records, its wall clock, its shard and the commit —
and the `coverage` job reconciles them.

```
npm test                                   every suite, serially, as it always has
npm test -- --shard=2/6 --receipts=out      one packed slice, receipts into out/
node run.js --coverage=out                  reconcile every shard's receipts
node run.js --suite=splash.js --receipts=out  one suite alone, its receipt rewritten in out/
node run.js --plan=6                        print the packing and exit
```

The nightly's `gates/ui-suite` uses `--suite` for its one solo attempt: a suite the sharded
run left red runs again alone after every shard has finished, then `--coverage` reconciles
the same receipts. The build is refused only if the suite fails alone too and the run log
quotes both attempts (the rule `run-tests.sh` has for a script suite).

**`coverage` is the job to read and the one to mark required.** A shard says only what its
own slice did; the sentence about the directory belongs to the job that has seen every
receipt and the shards say so themselves rather than leaving it to be inferred. Shard job
names also change the moment the shard count does, which is the second reason not to require
one.

It refuses unless the union of receipts **equals** the inventory discovered from disk, every
receipt carries **the same commit** and it is this checkout's, no suite is receipted twice
and the evidence gate passes over the aggregate. A suite whose shard never started has **no
receipt**, which is a named failure — not the same thing as a suite that ran and failed and
deliberately not the same sentence.

**The weights in `suite-weights.tsv` are never load-bearing.** They decide which shard a suite
lands in and nothing else, because coverage is proven from receipts — a stale weight costs
wall clock and cannot cost correctness. A suite with no weight is packed as the heaviest known
and named in the plan, so adding a suite needs no edit here, in the weights or in the YAML.

**macOS, not Linux and that is the expensive choice on purpose.** Playwright's Linux `webkit`
is the WebKitGTK port with a different graphics stack; on macOS it is a build of Apple's
WebKit, the engine family WKWebView renders through. An ubuntu runner would cost a tenth of
the minutes and would be answering a different question than rule 2 asks.

**`realbytes.js` runs there and that is measured rather than hoped.** It needs `cargo run
--example timeline_payload` from `richos/app/src-tauri` — the detached workspace with the whole
webview dependency tree behind it, which `app-spine-ci.yml` keeps off its test path on
purpose and which was estimated at 1.3 GB and about a quarter of an hour from cold. The
estimate bought it an `--allow-skip=realbytes.js` on the workflow's command line and the
estimate was never checked. Run 33872963879 checked it: the image carries cargo, the suite
ran and the gate reported the allowance as unused. 2m16s of a 25m10s job against a
45-minute budget. The allowance is gone and NO suite may skip — which matters most for this
one, because it is the only suite that renders the bytes the backend actually emits.

**And these suites still drive WebKit through Playwright, not the Tauri shell.** §23 Phase 6
— every acceptance state in the real shell — is not closed by any of this and the workflow's
name and output do not claim it is.
