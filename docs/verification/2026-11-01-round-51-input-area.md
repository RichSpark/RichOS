# Round 51's input area on the real app (2026-11-01)

The CEO's order: "implement `richos-hq/design/mockups/rounds/round-51`". This is the real-app check of
branch `cc/echo-opus-build51` at `e6f9cc55d`: the app built from the branch (`branch-bundle.sh`,
engine 1.2.0 pinned), run in a fresh test VM guest by `run-walk.py` with an empty home and driven
by `steps/walk.json` through `steps-walk.py`. The guest's first run added the company Acme and
opened its Onboarding conversation; every state below is that conversation.

## The seven states, both themes

Each picture is the app's window in the guest (left) beside the round's own capture (right).

| State | Dark | Light |
|---|---|---|
| 1 Empty | `1-empty-dark.jpg` | `1-empty-light.jpg` |
| 2 Two lines | `2-typing-dark.jpg` | `2-typing-light.jpg` |
| 3 Long | `3-long-dark.jpg` | `3-long-light.jpg` |
| 4 Files | `4-attach-dark.jpg` | `4-attach-light.jpg` |
| 5 Rich working | `5-working-dark.jpg` | `5-working-light.jpg` |
| 6 Talking | `6-talking-dark.jpg` | `6-talking-light.jpg` |
| 7 Plus | `7-picker-dark.jpg` | `7-picker-light.jpg` |

What matches the round in every frame: the wide box; the two-line empty box with "Type here… OR…"
and Talk to Rich at the start of line two, over the plus and the caret in front of "Type here";
the row with the plus, Talk to Rich once there are words, the status line, Output centered and Send
in the corner, filled when there is something to send; growth to ten and a half lines with the top
line faded; the files inside the box above the words; Stop alone in the corner while Rich works;
the listening panel with the pressed button where the placeholder had it and "← Click to stop
talking · headphones recommended".

The guest's accessibility tree shows Talk to Rich (`AXCheckBox "Talk to Rich"`) at x 477, the
plus (`AXButton "Add files"`) at x 477, so the button is reachable and sits over the plus.

## Where the app differs from the captures and why

- **The status line is empty and Output has no count.** A fresh install has no team work and the
  conversation has produced nothing yet; the mockup's frozen window carries a demo account's data.
- **The window is 1400 × 864** on the guest's 1680 × 1050 screen; the captures are 1440 × 900.
- **Plus (7):** the app opens macOS's own file panel, titled "Add files". It offers files only:
  the attachment desk refuses folders and the dialog plugin has no files-and-folders mode. The
  round's stand-in listed folders and its tooltip says "Add files and folders"; the app's says
  "Add files".
- **Talking, dark (6):** the guest was still checking its speech model when the button was
  pressed, so line one shows that row ("Checking my speech model…" with its bar) instead of
  "listening…". The round's README leaves other voice states to main's own words on the same line;
  line two and the button's place are the round's. The light frame shows "listening…".
- **Rich working (5):** the walk sent one real message; the conversation took its own title.

Nothing else differs.

Privacy: `scripts/qa/ocr-gate.sh` read the guest frames and the captures before these pictures were
made. It flagged the date in the mockup's synthetic file name "Screenshot 2026-11-01 at 9.14.png"
(phone-shaped) in `r51-picker-*.png`, so the picker pictures use copies with that one name covered
by `scripts/qa/redact.py --phones`; the gate then read 0 of 32 frames with a hit and 0 of 2 text
files. The app was quit and the guest stopped and deleted by `run-walk.py` at the
end of the run (`clean: app quit, VM stopped, clone deleted, state removed`).
