# `shots-splash/` — the two splash screens and the Settings row that holds their sound

Written by `../splash.js` out of WebKit's own compositor, every one decoded and
pixel-counted before it counted as evidence (`lib/harness.js`, rule 3). Overwritten on
every run and not byte-stable: read the suite's exit code, not a `git diff` over a PNG.

**There are exactly two splash screens**, ruled by the owner on 2026-09-01: those two are
approved and the other mockup designs are not approved for use as splash screens. This
directory used to hold thirteen material pairs and two composition shots of round-8.1 studies,
because the library shipped eighteen of them on the strength of an approval that was of a
**palette and a visual standard**, never of eighteen opening ceremonies. Those files are gone
with those entries. The ruling's own wording is in the private record.

**The composition shots stand at one moment.** Since round 74 the lamp drifts once the splash's first second is over and a pool of light follows it across the mat, so each is held (`RichSplash.holdAt`, the surface's own way of standing still) at a fixed moment for the exposure: the two compositions at the end of their three seconds, the material pairs at 2.2 s.

**The composition shots are taken with the surface HELD OPEN** and that is stated here
rather than left for someone to discover: the real splash lands at three seconds and then
hands off, so photographing it mid-run means muting the app-ready signal for the length of
the exposure. The suite's `holdOpen()` replaces only the exported `RichSplash.yieldNow`,
which is only what `main.js` calls — the compositions themselves are the shipped ones, drawn
by the shipped renderer from the shipped library and every assertion about *when* the
surface leaves (checks 8, 10, 12b, 12c) is made against the unmuted paths.

| Shot | What it is evidence of |
|---|---|
| `splash-01-round-11-v1.png` | **Splash screen #1**, `round-11/v1` — the ruled dark standard in the Sovereign palette, with the loading bar drawn as the `.rule` at the width of the plinth, striking along its own unstruck 22% ghost. Check 5 joins the mat's rendered value back to that entry's own `surface` token, so this is a photograph of the data, not of a copy of it. |
| `splash-02-round-11-v2.png` | **Splash screen #2**, `round-11/v2` — midnight suede, saddle-stitched, with the loading bar drawn as a leather strap of the same hide: the whole run of needle holes punched from the first frame, sewn live in gold thread at the plinth border's 10.5px pitch. The two shots differ because the two library entries differ; nothing in the renderer knows which is which. |
| `splash-03-the-sound-switch.png` | The splash screen's row in Settings, in the band across the panel's foot: "Play its sound" and the way into "Your openings". Until 2026-11-14 this row held the splash's off switch (`splash-03-the-off-switch.png` and `splash-04-a-launch-with-it-off.png` for the plain launch after it); the owner took the switch away that day ("the splash screen is now needed for the illusion of a super-fast app launch") and gave the row to the sound. Taken on a start whose old local mirror says the splash was switched off: the splash played. |
| `material-round-11-v1.png`, `material-round-11-v2.png` | Each screen's plinth as the **SHIPPING renderer** draws it on the left, beside the plinth **the round-11 mockup that entry names** draws on the right — the whole thing at half scale over the same top-left corner at native scale. |

## Why the pairs are pairs

A composition can be *reduced* rather than *reproduced*. #2's suede, its nap and its saddle
stitching would still pass every assertion about tokens while rendering as a flat navy
rectangle and that would be a lie about what the CEO approved. So check 18 puts the two
plinths side by side and check 15 measures the same claim as a number: emptying an entry's
material stack has to change at least a quarter of the mat and the mat has to differ by at
least as much from the plain one it would otherwise be a rename of.

The composition either side of the mat is identical by construction — same geometry, same
ink, same gold, same rule — which is why these crops are mats and not windows.

**WHERE THE SHIPPED SCREENS DIFFER FROM THEIR MOCKUPS, named rather than left to be found.**

* **#2's loading strap is SVG, not `<canvas>`.** The mockup draws every needle hole and
  every stitch on a canvas, per frame, from a seeded generator. Two reasons that could not
  ship: `tests/contrast.js` check 14 asserts **zero** `<canvas>` elements on any walked
  surface, so that a green contrast run cannot be read as covering pixels no DOM checker can
  see; and a canvas is code and the renderer holds no variation's values. The shipped strap
  is a 126px x 19px SVG tile of twelve stitches generated from the mockup's own `mk(41)`
  with its own jitter and its own three-stroke thread. The cost: the jitter repeats every
  twelve stitches instead of never and a stitch is revealed by the fill's edge crossing it
  rather than seated over its own ~28ms. At 10.5px over three seconds neither reads.
* **#2's brush prewash** — sixteen seeded strokes that mottle the nap and the pointer that
  keeps brushing it — is not carried. The nap is; the mottling is not, so the shipping suede
  is slightly more even than the mockup's. The mockup's lamp also tracks the pointer; the
  splash's is static, at the point the composition's own lamp sits, because nobody moves a
  pointer over a splash screen.
* **The mockups replay themselves.** They carry `HOLD_AFTER` and `AUTO_REPLAY` so a viewer
  can judge the run more than once without reloading. `AUTO_REPLAY` is `false` in both and
  nothing of the sort exists in the app at all: CEO, 2026-09-01, *"The animation is only
  supposed to happen ONCE. NO LOOPING."* Check 22 proves it three ways.

The full per-version account of the earlier material reduction — including the round-8.1
studies that are no longer shipped — is in
`docs/verification/opening-screen-2026-08-30/material-reduction.txt`.
