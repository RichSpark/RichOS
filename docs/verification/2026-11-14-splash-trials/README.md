# The three splash trials (Sage's plan, section 7), 2026-11-14

Plan: `richos-hq/docs/plans/2026-11-14-existing-splash-under-half-a-second.md`, section 7. The CEO's
line: 380 ms is the maximum p95 from the click to the splash playing.

## What was built and where it ran

- **Build tree:** richos main `e2c4934c6`, merged (not committed) with Echo3's
  `cc/echo3-opus-launchfix4` at `952d0e9c9`, which the plan depends on. That branch supplies the
  window's ground color, the push (`first_frame::commit`) and the closed list. Then
  `trial-build.patch`: three hooks in `main.rs` plus `splash_trial.rs`, `splash_trial.js` and
  `ui/trial-splash.html` (the app's own `<head>` with only the two splash scripts). The trial code
  is throwaway and is not in this branch's source; it exists here only as that patch.
- **Walk 1:** a debug build (`branch-bundle.sh`). **Walk 2:** a release build
  (`tools/release-bundle.sh`, `CARGO_BUILD_JOBS=2`, because the host's CPU breaker stops a
  release-mode `rustc` above 3 cores twice for anything that is not the nightly). Walk 2 also
  carries the fixes found in walk 1 (listed under "Walk 1 to walk 2"). Walk 2's numbers are the
  results. Walk 1 is kept as the record.
- **Where:** the test VM (macOS 15.7.7 guest, 4 processors, 5 GB, 1680x1050 at **1x**), one guest
  per walk through `run-walk.py`, a synthetic fixture home and no engine. Nothing ran on this
  Mac's screen. Every launch goes through `open -n -a` (the LaunchServices route a Dock click
  takes). Walk 2 waited until the guest's 1-minute load was under 3 before its first launch
  (load 2.4 to 4.0 at each launch). Walk 1 started 2 minutes after boot, at load 41.
- **How time is read:** `tools/screen-frames` (ScreenCaptureKit, in the guest) stamps every new
  screen frame with the instant macOS displayed it, on the same wall clock as the launch request
  and the app's `[trial]` lines. "On the glass" is read from those frames, never inferred. Pictures
  carry a 6-cell index stamp (one white guard cell and five bits), so each recorded frame says which
  picture it shows.

## Trial 1: a full-size picture, pushed, then the main thread busy for 2 s

**Pass line:** the picture is on screen before the 2 s end; gives the time to attach it.

**PASS.** In all 20 release launches and all 10 debug launches the picture was on the glass
1,963 to 1,982 ms before the busy period ended (`evidence/walk2/analysis.txt`, `t1`). The control
without the push (2 launches per walk) put nothing on the glass until the busy period was over:
2,207 ms after the request against a busy end at 2,161 and 2,176 ms
(`evidence/frames/t1nopush-01-no-push-during-busy-1200ms.png` shows the bare desktop and
`t1-05-picture-during-busy-1200ms.png` shows the picture).

Time to attach (the first picture is the page-sized 1400x836 PNG): decode 7.3 to 13.0 ms, attach
0.2 to 0.5 ms. In launch 5, for example, the picture was asked for at 141.3 ms and pushed at
154.0 ms after the process started, so 12.7 ms. The same picture at 2x (2800x1672, the Retina pixel
count) decodes in 31.7 to 36.8 ms (the trial 3 2x launches). The plan's bound, "the first picture
is on the glass within 10 ms of being asked for", is not met by a PNG at either scale.

### The CEO's 380 ms (release build, 20 launches)

| From the launch request | min | median | p95 (nearest rank) | max |
|---|---|---|---|---|
| to the process starting | 6.2 | 9.4 | 19.1 | 21.5 |
| to the "pushed" stamp | 139.8 | 172.0 | 264.9 | 342.5 |
| **to the picture on the glass** | **165.0** | **198.2** | **296.5** | **361.3** |

Per the plan (section 6), a Dock click adds 41 to 54 ms before the request. So click to picture is
p95 337.5 to 350.5 ms. The single slowest launch is 402.3 to 415.3 ms. That is under 380 at
p95 and over 380 on the one launch. The plan's own nightly line says "any launch", which is
stricter than p95. Three launches in a row were slow (6, 7 and 8: 296.5, 361.3 and 271.6 ms). In
each the time went before the app's first log line: process start to the activation line was 261 ms
in launch 7 against 105 ms in launch 5 (`launches/t1-0{5,7}/launch.json` in `evidence/walk2/raw.tgz`, lines
stamped on arrival). That is the update step, framework start and AppKit, before any window code.
This data does not split it further. Echo3's launch line would, but it is written only when the
page takes over, after these runs had quit.

**What these numbers do not cover:** the guest renders at 1x. On a Retina Mac the first picture
has 4 times the pixels and a PNG of it decodes about 25 ms slower (measured above). With that
estimated in, p95 is about 375 ms (unverified). Dictionary was measured in the same guest by
the same frame method, one launch at a time: median 345 ms, p95 1,239 ms (n=10; 8 of 10 between
314 and 352 ms). The debug build's trial 1 was p95 407.6 ms (walk 1).

## Trial 2: a hidden web view draws splash #2 held at three moments

**Pass line:** its snapshots compared with the running splash at those moments; three pixel
comparisons.

**How:** the main window ran splash #2 for real. At three moments the page froze it where it
stood: the clock fixed and every animation inside `#splash` paused. The record gives the exact
time reached (0, 759 and 850 ms; the 400 ms target was missed because the busy page delayed the
freeze). The guest captured the page from the screen (`screencapture -R`). Then a web view that
was never put on screen (`visible(false)`, `document.visibilityState` "hidden") loaded the
app's own stylesheets with splash #2. It was held at exactly those values and WebKit's
`takeSnapshotWithConfiguration` drew it. Both pictures were matched to sRGB and the screen
capture's 28-point title bar strip was cropped away.

**PASS, with the tolerance below** (`evidence/walk2/analysis.txt`, trial 2 lines):

| Moment | Hidden vs the screen: mean diff | PSNR | pixels off by more than 8 levels | Hidden vs the visible window's own snapshot |
|---|---|---|---|---|
| 0 ms | 0.19 levels | 46.1 dB | 0.27 % | identical, byte for byte |
| 759 ms | 0.78 | 40.4 dB | 1.01 % | 0.009 % off by more than 8 levels |
| 850 ms | 0.74 | 39.2 dB | 1.01 % | 0.056 % off by more than 8 levels |

- The hidden web view draws exactly what WebKit draws for the visible window. At 759 and 850 ms
  the only difference is the loading bar's leading edge
  (`diff-hidden-vs-visible-snapshot-2.png`). That comes from how this trial reproduces the bar's
  clock, not from the view being hidden.
- What separates any WebKit snapshot from the glass is the same for the visible window's own
  snapshot: one-level dithering in the soft gradients everywhere and antialiasing at the edges of
  the elements that are mid-rise (up to 72 to 97 levels, on edge pixels only). At 0 ms the 0.27 %
  is the window's own 1-pixel border, which the page does not draw
  (`diff-hidden-vs-screen-{0,1,2}.png`, differences multiplied by 16). At the take-over this
  could show as a sub-pixel edge change on the moving mark for one frame (unverified; the plan's
  frame-by-frame reading of route 1 is the check).
- The hidden page got no animation frames at all (`requestAnimationFrame` never fired; 18 timer
  fallbacks). The splash's bar is painted on animation frames, so the plan's "draw held at a
  moment" in `splash.js` must paint directly and never wait for a frame.
- **Drawing the opening at quit, measured:** 30 moments (the whole page once, then the changing
  region of 850x794 points at each moment, written as PNG and JPEG) took **4.03 s**. That is over
  the plan's 3 s bound (section 1). Holding the 30 moments took 0.34 s, WebKit's drawing 2.57 s
  (median 73 ms a snapshot) and writing the files 1.01 s, all at 1x. Walk 1's 12.65 s was the
  trial's own timer waits and is superseded. The hidden page was ready 150 ms after it was asked
  for.

## Trial 3: trial 1 with 30 saved pictures played by Core Animation while the main thread is busy

**Pass line:** the recording read frame by frame shows every picture on time; memory and read time noted.

**How:** the first picture is the whole page, as in trial 1. The later ones are the changing
region (850x794 at 1x; 1700x1588 in the 2x set, close to the plan's 1,640x1,520 estimate for his
Mac). They are read on 4 threads in the order they are needed and handed to Core Animation in runs
of 3 as they arrive, each run a discrete `contents` keyframe animation timed from the push. The
packing is the engineer's call per the plan (section 1). The 2x set is walk 1's pictures upscaled
on the host: it stands in for decode cost and memory, because the guest cannot render at 2x.

| Variant (3 launches each) | All 30 shown in order, none later than its slot | Read and decoded (30 pictures) | Decoded size | App memory at most |
|---|---|---|---|---|
| 1x PNG | **yes**, all 3 | 196 to 274 ms | 81 MB | 214 to 221 MB |
| 1x JPEG (quality 0.92) | **yes**, all 3 | 60 to 75 ms | 81 MB | 229 to 240 MB |
| 2x PNG | **no**: launch 1 skipped pictures 1, 3, 4 and 15; launch 2 skipped picture 1 | 619 to 828 ms | 324 MB | 485 to 495 MB |
| 2x JPEG (quality 0.92) | **yes**, all 3 | 191 to 242 ms | 324 MB | 564 to 573 MB |

Trial 1 alone peaks at 113 to 128 MB. "On time" is measured from the first picture on the glass.
Each picture appeared at or before its 1/30 s slot, give or take one 60 Hz refresh
(`lateness_ms_by_picture` per launch). A skipped picture is one whose slot had passed before its
run was handed over (2x PNG launch 1: the run with pictures 15 to 17 arrived 8 ms late). Walk 1
handed all 30 over at once after the last decode (165 to 462 ms after the push), which by
arithmetic loses the first slots. Walk 1's region layer was also placed 12 points low (AppKit
resets `geometryFlipped` on a layer-hosting view), so its trial 3 recordings could not be read.
Both were fixed in walk 2. JPEG's fidelity against the splash's own drawing was not measured
(that is the plan's per-entry check, section 6).

## What the plan says follows

- **Trial 1 passed.** The push shows the window while the main thread is busy, so the audit's
  piece 2 does not come back.
- **Trial 2 passed.** A hidden web view gives true pictures (the tolerance is above), so the
  opening can be made ahead of a splash nobody has seen yet. Two findings feed the build: drawing
  must not depend on animation frames and 30 moments took 4.03 s against the 3 s bound.
- **Trial 3 passed at 1x and at 2x with JPEG. It failed at 2x with PNG.** For a failure on size or
  timing the plan says "the opening gets fewer pictures per second or covers a shorter time and
  section 4's steps to bring the page in sooner become required". At his Mac's pixel count the
  same recording passes with JPEG at the same 30 per second and the same length, at about 570 MB
  of app memory while it plays. That is a packing choice the plan leaves to the engineer. The
  first picture's own decode (32 to 37 ms at 2x as PNG) is over the plan's 10 ms bound in every
  variant measured.

## Surfaces

macOS app started through LaunchServices, a returning user's launch (the trials force splash #2
and a fresh launch where needed), in the test VM only. Not covered: a Retina display (2x is a
stand-in for decode and memory only), his Mac's own load, a launch after an update, light theme
and reduced motion (the splash is dark-only; reduced motion was not set), Windows. All test data
was synthetic and inside disposable guests.

**Instances:** walk 1 started 27 app or Dictionary instances, walk 2 started 45. Every one was
quit by the pid captured at launch and checked gone with its children. `left_running` is empty in
both walks' `facts.json` (in `raw.tgz`). `run-walk.py` quit its own instance, stopped each VM and deleted the
clone ("clean: app quit, VM stopped, clone deleted, state removed").

## The evidence here

- `evidence/walkN/analysis.txt`: every launch's row as `analyze.py` read it, plus the statistics.
- `evidence/walkN/raw.tgz`: the walk's own records. It holds each launch's app lines stamped on
  arrival, its pid and quit, its memory samples and the recorder's per-frame display times
  (`launches/*/`), `facts.json`, `summary.json`, `run-walk.py`'s report and trial 2's freeze
  records, requests and opening description (`trial2/`). The raw gray frames (2 GB a walk) are
  not kept.
- `evidence/walk2/trial2/`: the three moments from the screen, from the hidden web view and from
  the visible window's own snapshot. Also the differences multiplied by 16, the first picture of
  the opening and one region picture as PNG and as JPEG.
- `evidence/frames/`: recorded instants of walk 2 as pictures (`tools/frame-png.py`).
- `trial-build.patch`: the throwaway trial code. `tools/`: everything that ran it, covered by
  `richos/app/scripts/splash-trials-tools.test.sh`.

## Walk 1 to walk 2

The release build. The picture is placed where the page draws, under the title bar. Snapshots are
cropped to the page. The region layer is placed in AppKit's bottom-up coordinates. The hand-over
is in runs, in the order needed. The hidden page's timer waits went from 120 ms to 1 ms. The
first picture is drawn whole once, then only regions. The guest settles to load 3. Trial 1 has 20
runs. The 2x set was added.

## Reproducing

```sh
R=<richos worktree>; T=$R/richos/app/scripts/testvm; D=$R/docs/verification/2026-11-14-splash-trials/tools
git -C $R merge --no-commit cc/echo3-opus-launchfix4 && git -C $R reset -q
git -C $R apply docs/verification/2026-11-14-splash-trials/trial-build.patch
. ~/.richos-nightly/releases/v1.3.2-nightly.20261114.99/engine-pin.env
CARGO_BUILD_JOBS=2 python3 $T/reserve.py --wait 1200 -- bash $D/release-bundle.sh $R/richos/app --out RichOS-trial-release.zip \
    --engine-version "$RICHOS_ENGINE_VERSION" --engine-url "$RICHOS_ENGINE_URL" --engine-sha256 "$RICHOS_ENGINE_SHA256"
swiftc -O -target arm64-apple-macos14.0 $D/screen-frames.swift -o screen-frames
python3 $T/launch-nothing-walk.py --make-home home
python3 $T/run-walk.py --wait 1800 --bundle RichOS-trial-release.zip --home home \
    --engine ~/.richos-nightly/releases/v1.3.2-nightly.20261114.99/richos-engine-1.2.0.tar.gz --report report.json -- \
    $D/splash-trials-walk.py --testvm $T --frames screen-frames --opening2x opening2x --out walk
tar xzf walk/out.tgz -C walk && python3 $D/analyze.py walk/out
```

The 2x set is walk 1's `opening/region-*.png`, upscaled with `sips -z 1588 1700`, plus a JPEG of
each (`sips -s format jpeg -s formatOptions 92`) and `full-000.png` cropped to the page and
scaled to 2800x1672. `tools/frame-png.py` turns any recorded instant into a picture.
