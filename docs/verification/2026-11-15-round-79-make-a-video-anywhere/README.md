# Round 79, "Make a video" anywhere in the app, on the real app (2026-11-15)

The CEO's order: "Get someone on Fable to build round 79." This is the check of branch
`cc/echo2-fable-r79build1`: the app built from `50b60fe63` (`branch-bundle.sh`, engine 1.2.0 pinned
as nightly 100 pins it, bundle sha256 `6bdc6a44…d1c1e7`), run in a fresh test VM guest by
`run-walk.py` and driven by `richos/app/scripts/testvm/video-anywhere-walk.py`. Nothing ran on the
host's screen. No sound was played and no microphone was opened: the guest has no sound device and
the app was started with `RICHOS_VIDEO_INPUT_TONE=1`, so the "Microphone" choice recorded a tone
made in the page.

## What the guest shows

| Step | Result | Frame |
|---|---|---|
| The button | In the conversation, nothing of the recorder is on the screen until Settings is open. "Make a video" stands in the heading of Settings at its right end (160 × 36 at 1328,166; the panel is 1070 wide from 455, so 37 px from its right edge) | |
| The choices | Microphone (Off, On) and Mouse pointer (Shown, Hidden); as first opened Off and Shown; no "Recording stops", "By itself" or "When I stop it"; Escape closes them and Settings stays | `guest-2-choices.jpg` |
| Start recording | Settings closes for the count; the red Stop with the time stands beside the settings button (7 px to its left, level with it) | |
| A line highlighted | On the RichPass page the real mouse is dragged over "Safer Than Any Password Manager"; the recording goes on (0:19) | `guest-6-highlighted.jpg` |
| Settings while it records | The heading of Settings holds the same red Stop with the same time (0:26); Escape closes Settings and the recording goes on | `guest-7-in-settings.jpg` |
| The home screen | Still recording (0:40); Stop stands where the home screen's own "Make a video" stands, which is out of its way | `guest-8-home.jpg` |
| Stop, the sheet | "Your video", "0:42 · with your voice · with the mouse pointer · 1,400 × 836 · 0.4 MB", over the home screen, which stays | `guest-9-sheet.jpg` |
| Save video | The Mac's own save window, then "Saved to Documents as RichOS-2026-11-15-at-13.02.56.mp4"; Done puts the home screen's own button back | `guest-10-anywhere-saved.jpg` |
| The saved file | In the guest at `/Users/admin/Documents/RichOS-2026-11-15-at-13.02.56.mp4`, 416,149 bytes (the app's log names the same path and size). Read back on the host: AAC, 48 kHz, mean volume −27.6 dB (the tone, not silence), decodes with no error, 28.91 s | `walk-report.json.txt` |

`run-walk.py` quit the app by its pid (1668), stopped the VM and deleted the clone.

## What the guest cannot show and where it is shown instead

**The picture in the file and the whole length of the sound.** The guest has no hardware H.264
encoder and the webview's recorder then writes the sound and no picture
(`docs/verification/2026-11-14-round-77-make-a-video`). In a guest the sound also ends before the
recording does: 28.91 s of sound for the 42 s above and in two earlier guests 10.03 s for a
recording of 48 s and 10.03 s for one of 27 s in which nothing was pressed. Why it ends early there
is not established.

The picture and the length are proved on this Mac's own hardware, with no window, no screen and no
sound, by `richos/app/ui/tests/video-anywhere.js`: the same `video-anywhere.js` records in headless
WebKit, each recording is saved with the sheet's own button and the bytes the save window is handed
are read back with ffprobe and ffmpeg.

| What | In the file |
|---|---|
| A recording from a chat into Settings, to the RichPass page and to the home screen | H.264 1440 × 900 + AAC; at each screen the file's frame is nearer to a picture of the window there than to any other screen (mean difference 3.0, 6.9, 4.5 and 3.0 against 10.2 at the nearest wrong one) |
| The mouse pointer; the recorder's own red Stop | The arrow is in the frame where the mouse was; the red button is 388 pixels in the window and 0 in the file |
| A line of the conversation, highlighted | 12,035 of 15,666 pixels of the line's box in the highlight color while selected, 193 before |
| A draft of four lines with the highlight on the last; an address broken over lines | Each third of both boxes has the ink the window has, within 8 pixels (2,355 against 2,356 in the first third of the address); the highlight is in the lower half of the message box (2,790 pixels) and not the upper (33) |
| A recording longer than ten seconds | 12.1 s in the file for 0:12 on the sheet; a recording of 30 s gave 30.10 s of H.264 and 30.12 s of AAC |

`host-file-frame-highlight-light.jpg` and `host-file-frame-highlight-dark.jpg` are frames taken out
of two saved files with ffmpeg: two lines of the conversation highlighted by a drag of the mouse, the
pointer at the end of the drag, a draft of two lines in the message box and the top bar empty
where Stop stood.

The suite is red on main `8cad38a78` (0 of 7 checks, as it was first written) and green on the
branch (8 of 8).

## What the recording does not show

The window is redrawn for the video, not photographed (the CEO, 2026-11-15, asked whether to keep
the redraw or capture the real window: "Yes. Keep the redraw with highlights."). Left out:
gradients behind plates; the blinking text cursor; tooltips the Mac draws; a select's small arrow;
a playing video; a PDF shown in the Output panel; the previews of files in the Output panel (a
quiet plate stands in for each: they come from another place than the window's own and a canvas
that drew one could no longer be recorded); and a one-line field scrolled sideways, which is drawn
from its start.

## Not checked

- A file written by the app's own WKWebView on a real Mac: its picture, its length past ten
  seconds and the app's own pictures (Rich's face, the tools' icons) drawn in it. The host suite
  runs Playwright's WebKit, where a page opened from disk may not read its pictures, so there a
  quiet plate stands in for each. What would settle it: one recording of half a minute made in the
  app on a Mac and opened.
- A real microphone and the Mac's own question when "On" is first pressed.

## Where the app differs from the round

- Text he highlights is in the video (his order of 2026-11-15, after the mockup); the mockup's
  recorder left it out.
- Save video opens the Mac's save window and the sheet names the folder he chose.
- Every line of a text box of several lines and of a word broken over lines is drawn.
- On the home screen the keyboard stays with the recorder's pieces, so Enter on Save video does
  not open Talk to Rich; Space on the sheet's player pauses and plays it.
- "Record again" on a home screen that has no "Make a video" of its own opens these choices under
  the settings button.
- The count and the sheet stand under the settings button, not over it (the app's rule that the
  button is above every overlay) and every size of type follows Text size.
