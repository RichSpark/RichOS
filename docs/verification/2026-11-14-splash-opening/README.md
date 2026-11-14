# The splash picked at quit and played at the click, measured in the test VM, 2026-11-14

Plan: `richos-hq/docs/plans/2026-11-14-existing-splash-under-half-a-second.md` (Sage). The CEO: "380 ms
as the maximum allowed p95 time for the RichOS app to start showing the splash screen." Built on the
three trials (`docs/verification/2026-11-14-splash-trials/`, branch `cc/echo2-opus-trials2`).

## What ran

- **Candidate:** this branch's release build, `testvm/branch-bundle.sh --release` at `8832522ce` (the app
  code has not changed since), version 1.3.3, ad-hoc signed. **Older copy** for the after-update route:
  the same app code built as 1.3.2 (`bd7ae7f58`). **Today's stable:** the published
  `RichOS-1.3.2-macos-aarch64.zip`, with nightly `1.3.2-nightly.20261114.99` as its older copy.
- **Gate:** `testvm/launch-speed-walk.py` through `run-walk.py --no-app --no-audio`, one fresh guest each
  (macOS 15, 4 processors, 1680x1050 at 1x), 20 launches a route, every launch through `open -n -a`
  (LaunchServices, the Dock's route). Times from the app's launch line and the guest's system log, plus
  71 ms for the Dock's own part of a click (54 ms, the slowest measured on his Mac) and one refresh.
- **The VM's slowness, like for like (the CEO's rule; the lead's decision (b) for the cold route):**
  Dictionary 10 times warm and 10 times after its own `sudo purge`, in the same guest; each median from
  the launch request to its window, plus the Dock's part measured on his Mac (median 43.5 ms of 44, 41,
  43, 54), over Dictionary's median on his Mac from the Dock click (318 ms of 287, 305, 318, 318, 328,
  380). Recorded beside the limit in each walk's `report.json` (in `walks.tgz`: `build/`, `stable/` and the smoke walk's `smoke-report.json`) (`summary.guest_inputs`, `summary.route_limits`).
- The host carried other agents' work throughout (load 8 to 18); both walks ran in the same conditions.

## The verdicts, once each

| Route (20 launches unless said) | **this build** p95 / median | today's stable p95 / median |
|---|---|---|
| ordinary (a returning user's launch) | **320 / 237 ms** | 17,857 / 10,463 ms |
| rule switch (route 5; 40 launches: start 2 shows #2, start 3 #1) | **359 / 254 ms** | 27,428 / 8,210 ms |
| first launch after an update put in place at quit (route 3) | **296 / 223 ms** | reported, not judged (its older copy predates the saved pick) |
| first launch after the Mac starts (route 2, each after its own purge) | **3,284 / 2,280 ms** against its own limit of 5,233 ms | 23,195 / 12,104 ms against 6,884 ms |
| **every warm route together** | **p95 357 ms over 80 launches, limit 857 ms: PASSES** | p95 23,453 ms over 60 launches, limit 1,009 ms: **REFUSED** |
| each splash today's rule picks | round-11/v2 p95 518 ms (20); round-11/v1 p95 320 ms on the warm routes | none timed by splash (it writes no launch line) |

- Warm limit: 380 x 2.26 = 857 ms in the build's guest (Dictionary median 674 ms from the request +
  43.5), 380 x 2.65 = 1,009 ms in the stable's. Cold limit: 380 x 13.77 = 5,233 ms (Dictionary after a
  purge, median 4,336 ms from the request: 2,355 to 8,130) and 380 x 18.12 = 6,884 ms.
- **Measured against 380 ms unscaled**, the build's warm routes are within it too: ordinary 320, rule
  switch 359, after-update 296. round-11/v2 alone (its 20 launches, all the second start of a fresh
  home) has p95 518 ms: over 380, under the scaled 857.
- The stable's walk ran with the route of an older copy in /Applications handing over (dropped by the
  lead the same evening); the stable walk's `rejudged.json` inside `walks.tgz` is its records re-judged by the committed gate without
  that route (`cd734ca94`: a build older than the launch line is timed by WebKit's document loaded, not
  refused for not naming its splash). Refused either way: the walk's own report refused it as incomplete.
- Exit codes: the build's walk 0, the stable's 1.

## What the build's launches show

- **The opening was used on 98 of 100 judged launches**, 5 to 30 later pictures each. The two that were
  not: one cold launch whose window came up 2 points taller than the window its quit had drawn for
  (1400x864 against 1400x866) and one rule-switch start whose quit ran out of time (3,078 ms, "no
  opening was saved (out of time)"); both said so in the launch line and showed the splash's ground
  until the page drew the splash.
- **Drawing at quit: 43 drawings, 1,014 to 2,786 ms, median 1,872 ms**, under the plan's 3 s bound;
  under the host's load the coarse-first order kept the whole second at fewer pictures (5, 8, 15, 21, 25
  or 30 of 30). The trials' 4.03 s for 30 pictures in order no longer applies.
- **Inside the process, warm:** window 118 ms, first frame 159 ms after the kernel started it (medians);
  the page joined the opening at its end (1,000 ms of the splash) on most launches, so the opening holds
  its last picture until the page takes over 1.5 s after the process start, as the plan's section 2
  provides ("the splash then stands still for the difference").
- **Cold (each after a purge):** the process starts 493 ms after the request (LaunchServices and exec on a
  flushed disk), then window 1,410 ms and first frame 1,748 ms after its start (medians; in the smoke
  walk's launch lines: update step 193 to 295 ms, window 1,767 to 2,079 ms). The splash's own work is
  the step from window to first frame (about 35 ms); the cold time before it is the update step's file
  reads and the Tauri, AppKit and WebKit start-up paging in from a flushed disk. Not split further here.
- **It is the splash and it is playing:** `build/frame-0.45s.jpg` and `frame-0.70s.jpg` (an ordinary
  launch, a quarter second apart): the splash rising, 15.7 % of the middle of the screen moved, nothing
  white. The stable's pair shows its window not yet up (`stable/`), 0 % moved.
- Fallbacks, reported once each, not judged: the first start after an install shows the ground until the
  page draws the splash (the smoke walk's `fresh:first`); the first start after an update that changed the
  splash's own files ("the splash's own files changed since it was made (an update)") 1,585 ms.

## The splash itself, unchanged: the per-entry check and JPEG

- `ui/tests/splash.js` check 28: each catalog entry's opening (first picture with the changing part laid
  over it) against the same splash running, at 1024x700, 1280x800 and 1400x880: **0 levels off, on 0 %
  of the pixels**, in four runs. Checks 26 to 29 fail on main (no `holdAt`, no opening page) and pass here.
- JPEG at quality 0.92 (ImageIO, the app's encoder) on the changing part of held pictures from check 28:
  round-11/v1 PSNR 45.9 to 47.4 dB, mean 0.6 to 0.7 levels, worst 42 to 57, 0.2 to 0.5 % of pixels off by
  more than 8; round-11/v2 (suede) 40.4 dB, mean 1.1, worst 66 to 69, 2.1 %. At Retina pixel count (the
  crop scaled 2x, as the trials' stand-in): v1 49.7 dB, 0.09 %; v2 44.2 dB, 0.9 %. The first picture is raw
  pixels, byte for byte (`splash_opening::tests`).

## Not covered

A Retina display (the guest renders at 1x; 2x is a decode and JPEG stand-in only), his Mac's own load,
a real reboot (route 2 is a purge), the frame-by-frame reading of the take-over step (the page and the
opening are drawn by the same code and compared at 0 levels in check 28; the screen recording across the
moment was not taken), Windows.

## Instances

Every app and Dictionary instance in both walks was quit by the pid captured at launch and checked gone
(`instances` in each report, `left_running` empty); each `run-walk.py` ended "clean: app quit, VM
stopped, clone deleted, state removed". Synthetic fixture homes only. Nothing ran on this Mac's screen.
