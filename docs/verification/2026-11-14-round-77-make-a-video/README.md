# Round 77, "Make a video", on the real app (2026-11-14)

The CEO's order: "`richos-hq/design/mockups/rounds/round-77` is great. Get an engineer on Fable to
build it." This is the check of branch `cc/echo4-fable-videobtn1`: the app built from `2c6e42f22`
(`branch-bundle.sh`, engine 1.2.0 pinned as v1.3.2 pins it, bundle sha256 `0db36a0e…5b7d79`), run in
a fresh test VM guest by `run-walk.py --no-audio` and driven by
`richos/app/scripts/testvm/home-video-walk.py`. Nothing ran on the host's screen. No sound was
played and no microphone was opened: the guest has no sound device and the app was started with
`RICHOS_VIDEO_INPUT_TONE=1`, so the "Microphone" choice recorded a tone made in the page.

## What the guest shows

| Step | Result | Frame |
|---|---|---|
| The button | "Make a video" at the top right, level with the settings button and 11 px to its left (164 × 40 at 1307,123; settings at 1482,123) | `1-button.jpg` |
| The choices | Microphone, Recording stops and Mouse pointer; as first opened Off, When I stop it, Shown; Escape closes them and the home screen stays | `2-choices.jpg` |
| Microphone On, When I stop it, Shown | The count with "Your microphone is on"; the red button "Stop 0:04" with the bars; the sheet "Day 104 · 322 memories", "0:10 · with your voice · with the mouse pointer · 1,400 × 836" | `count-recording-save-window-saved.jpg`, `film-no-file-sheet-microphone-sheet.jpg` |
| Save video | The Mac's own save window, titled "Save video", suggesting `RichOS-day-104-322-memories`; Save; the sheet says "Saved to Documents as RichOS-day-104-322-memories.mp4" and offers Record again and Done | `count-recording-save-window-saved.jpg` |
| The saved file | In the guest at `/Users/admin/Documents/RichOS-day-104-322-memories.mp4`, 112,672 bytes (the app's log names the same path and size). Read back on the host: AAC, 48 kHz, 10.39 s against the sheet's 0:10, mean volume −27.5 dB (the tone, not silence), decodes with no error | `walk-report.json.txt` |
| Microphone Off, By itself, Hidden | The film plays once with the numbers, the team and the buttons off the screen ("First Color · Day 30 · 122 memories", Stop 0:07) and the recording ends by itself | `film-no-file-sheet-microphone-sheet.jpg` |

## What the guest cannot show and where it is shown instead

**The picture in the file.** The guest has no hardware H.264 encoder: a probe run in a guest
(macOS 15.7.7) got status −12903 from VideoToolbox when hardware is required and a session when it
is not. The webview's recorder then writes the sound and no picture. So the file saved with the
microphone holds the AAC track alone and the film, which has no sound, leaves no file: the sheet
says "This Mac could not record the window, so there is no file to play here." with Record again
and Discard (same frame as above). Every Mac a user runs has the encoder.

The picture is proved on this Mac's own hardware, with no window, no screen and no sound, by
`richos/app/ui/tests/home-video.js`: the same `home-video.js` records in headless WebKit, each
recording is saved with the sheet's own button and the bytes the save window is handed are read
back with ffprobe and ffmpeg.

| Recording | File | Length | Size |
|---|---|---|---|
| Microphone (the tone), stopped by the button, pointer shown, 312 memories | H.264 1440 × 900 + AAC | 4.1 to 4.4 s for about 4 s | about 2 MB |
| The film, no sound, pointer hidden, 312 memories | H.264 1440 × 900, no sound | 16.0 s (0.5 s on the empty picture, 12 s of film, 3.5 s on the last frame) | 7 to 10 MB |

The pointer is in a frame of the first (a white-edged arrow at the place the mouse rested) and in
none of three frames of the second; the recorder's own button is in neither. Frames out of one film
file, at 1, 8, 13 and 15 s, are in `host-film-file-frames.jpg`: one memory, 140, then the last frame
standing with "Day 67 · 312 memories · The River".

The suite is red on main `8400f8f12` (0 of 8 checks) and green on the branch (9 of 9, twice back to back).

## Not checked

- An MP4 written by the app's own WKWebView on real hardware and the video playing in the sheet
  there. The host suite runs Playwright's WebKit, not the system's.
- A real microphone and the Mac's own question when "On" is first pressed. The order was a made-up
  input; the `getUserMedia` path was exercised only against the suite's stand-in.

## Where the app differs from the round

- Save video opens the Mac's save window and the sheet names the folder he chose, where the mockup
  says "Saved to Downloads" (the round's README says so itself).
- The sheet's window is 1,400 × 836 in the guest; the round's captures are 1,440 × 900.
- "…to today and the recording stops…" has no comma before "and" (the app's rule of 2026-10-19).
- The microphone is let go whenever he is not choosing, counting or recording; his choice stays.
- The settings button is not in the video and neither is the pointer while it rests on it.
