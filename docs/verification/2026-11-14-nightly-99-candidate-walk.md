READY: nightly v1.3.2-nightly.20261114.99 (run 20261114T062748Z-214f92e3, source c9d4c4a71, build d16436c36). Evidence: `docs/verification/2026-11-14-nightly-99-candidate-walk/source-0039-second-press-in-view.png`. The second Source link, pressed with the Output panel already open, now brings its passage into view and keeps it there. `release-lines-walk.py` `stop-notes: PASS`, with the [00:39] rows at y 533 and 620 inside the panel's 190 to 900 view, both 4 s and 12 s after the press.

# Nightly 99 candidate walk, 2026-11-14 (Ray)

**How it was tested.** I used the candidate's own zip, `RichOS-1.3.2-nightly.20261114.99-macos-aarch64.zip`. `shasum -a 256 -c SHA256SUMS` reported every file OK. The app ran only in the test VM, through `run-walk.py`: headless, one guest, the guest screen at 1680x1050, one window and no second user. That was one run and its report says `cleanup_complete: true`. Every claim below was **checked on the live app in the VM** by reading its screen and its accessibility tree.

**Identity.** The `identity` step passed: the app says it was built from `d16436c361e8940dc7828e37722d1e4e8f56993e`. The build's `build-info.json` maps that build commit to source `c9d4c4a7168b792e4be3f195e3ce57b0b67af0bc` and its parent is the merge `c9d4c4a71`. The build's own VM boot proof passed (`~/.richos-nightly/gui-proofs/20261114T062748Z-214f92e3.proof`: `result=pass`). This walk did not repeat it.

**Tools used.** These are committed tools from this branch (richos `c9d4c4a71`):
- `testvm/run-walk.py` and `testvm/release-lines-walk.py` (unchanged), steps `identity,companion,audio,first-run,prompt,ask-live,stop-notes`, run with `--no-audio`.
- `qa/speech-wav.py`, which rendered the meeting fixture to a file, so **nothing was played aloud** (CEO §53).
- `qa/wait-for.sh` and `qa/ocr-gate.sh`. The gate's positive control passed and 0 of 3 frames were flagged.

**Written from scratch: nothing.** The meeting script used as the fixture is at the end of this file. Copy it to a `.json` file to reuse it.

## The one item

1. **Source links in meeting notes, pressed one after another: PASS.**
   - **Before the first press, the Output panel was closed** (`source-links-before-press-panel-closed.png`).
   - **First press, from the closed panel: Source 00:28.** The panel opened on transcript.md with the [00:28] row at y 545, inside the 190 to 900 view. The reading was the same at 4 s and at 12 s (`source-0028-first-press-in-view.png`).
   - **Second press, from the open panel: Source 00:39.** The transcript scrolled to the [00:39] rows, at y 533 and 620. The reading was the same at 4 s and at 12 s. It did not jump back to the transcript's header, which is what nightly 98 did (`source-0039-second-press-in-view.png`).
   - "This source passage could not be located." never showed.
   - **What a user sees:** each Source link they press opens the transcript at the moment the link names, whether or not the panel was already open.

## Notes that do not block

- **Ask Rich during the recording.** Asked about the call while it was still recording, Rich said the live transcript was unavailable and did not guess. Nightlies 97 and 98 gave the same answer (the VM speed limit). This was not in this walk's scope.
- **Duplicated transcript lines.** With the guest's loopback microphone, every transcript line appears twice, once as "Me" and once as "Them". The notes say so ("every line appears on both your side and the other side"). This comes from the test rig, not from a person's call.

## Surfaces applied

Applied:
- The conversation with the meeting notes.
- The Output panel, both closed and open.
- The accessibility press of a Source link, the same press VoiceOver makes.

Not applicable:
- Dark mode: the fix changes scrolling, not color.
- The home screen, RichFiles and SwearMeter: they did not change since nightly 98, whose walk passed them.
- The phones: no `richos/mobile/**` change, so the battery check does not apply.
- Voice talk-over: no barge-in claim was tested.

## Test data and app instances

The test data was invented fixtures only, inside one disposable guest. Every name and company in the meeting fixture is invented. Nothing ran on this Mac's screen or in its own app data (§65).

`run-walk.py`'s stop quit the guest app (`quitting recorded app pid 1067`) and then reported `clean: app quit, VM stopped, clone deleted, state removed.` No `richos-tauri` was started on this Mac.

## The meeting fixture

This is the input to `qa/speech-wav.py`: voices Daniel, Karen and Moira, 34 lines, 159.4 s.
- **Decisions:** at 28 s (wholesale prices up 8% on March 1), 38 s (no grace period), 68 s (a second tin proof) and 122 s (flat $12 shipping under $200).
- **The booth line** is at 77 s.
- **Every name and company is invented.**

```json
[
  {"voice": "Daniel", "text": "Okay, let's get started. This is the Juniper Hill pricing call. Priya and Fiona are both here.", "pause": 1.0},
  {"voice": "Karen", "text": "Hi Sam. I have the cost numbers from the green coffee supplier ready.", "pause": 1.0},
  {"voice": "Moira", "text": "And I have the notes from the three wholesale accounts we talked to last week.", "pause": 1.0},
  {"voice": "Daniel", "text": "Good. Priya, how bad is the cost increase?", "pause": 0.8},
  {"voice": "Karen", "text": "Our green coffee costs went up eleven percent since spring. We are absorbing all of it right now.", "pause": 1.0},
  {"voice": "Daniel", "text": "Then we decided: we raise wholesale prices eight percent on March first.", "pause": 1.2},
  {"voice": "Moira", "text": "The cafes will ask for a grace period. Two of them already hinted at it.", "pause": 0.8},
  {"voice": "Daniel", "text": "No grace period. Everyone gets the same date. We give them notice now instead.", "pause": 1.2},
  {"voice": "Karen", "text": "Agreed. Sixty days of notice is fair.", "pause": 1.0},
  {"voice": "Moira", "text": "Next item is the new tin. The first proof from the printer had the logo off center.", "pause": 0.8},
  {"voice": "Daniel", "text": "Did they say why?", "pause": 0.6},
  {"voice": "Moira", "text": "They blamed our artwork file, but the file is fine. I checked it twice.", "pause": 0.8},
  {"voice": "Karen", "text": "Then we ask for a second tin proof before we approve anything. No print run until we see it.", "pause": 1.2},
  {"voice": "Daniel", "text": "Yes, we decided on a second tin proof. Fiona, you own that.", "pause": 1.0},
  {"voice": "Moira", "text": "Got it. I will call the printer tomorrow morning.", "pause": 1.0},
  {"voice": "Karen", "text": "One more thing. We got the spot at Seattle Coffee Fest in April. It is in hall B, booth forty two.", "pause": 1.2},
  {"voice": "Daniel", "text": "That is great news. Hall B is where the roasters were last year.", "pause": 0.8},
  {"voice": "Karen", "text": "We need the banner and the sample cups ordered by the end of January.", "pause": 1.0},
  {"voice": "Moira", "text": "I can design the banner if someone else orders the cups.", "pause": 0.8},
  {"voice": "Karen", "text": "I will order the cups.", "pause": 1.0},
  {"voice": "Daniel", "text": "Now the online store. Shipping is costing us more than we charge.", "pause": 0.8},
  {"voice": "Moira", "text": "Customers hate surprise shipping fees at checkout, though.", "pause": 0.8},
  {"voice": "Karen", "text": "What if we keep it simple, one flat rate below a certain order size?", "pause": 0.8},
  {"voice": "Daniel", "text": "How about free shipping over two hundred dollars, and a flat fee under that?", "pause": 0.8},
  {"voice": "Karen", "text": "Twelve dollars would cover almost every order we ship.", "pause": 1.0},
  {"voice": "Daniel", "text": "Then we decided: flat twelve dollar shipping on orders under two hundred dollars.", "pause": 1.2},
  {"voice": "Moira", "text": "I will update the checkout page once it is final.", "pause": 1.0},
  {"voice": "Daniel", "text": "And I owe everyone the new price list with the eight percent built in, by Friday.", "pause": 1.0},
  {"voice": "Karen", "text": "Send it to me first so I can check the margins.", "pause": 0.8},
  {"voice": "Daniel", "text": "Will do.", "pause": 0.8},
  {"voice": "Moira", "text": "Do we know yet whether the Portland cafe is renewing its contract?", "pause": 0.8},
  {"voice": "Daniel", "text": "Not yet. That stays open until they call back.", "pause": 1.0},
  {"voice": "Karen", "text": "Okay. I think that is everything for today.", "pause": 0.8},
  {"voice": "Daniel", "text": "Thanks, both of you. Talk next week.", "pause": 1.5}
]
```
