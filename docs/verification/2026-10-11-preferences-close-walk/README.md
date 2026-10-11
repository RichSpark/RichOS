# Closing Preferences saves Rich's model at once, with "Not now" still waiting (2026-10-11)

**What it proves:** in the built app, with the interview card's "Not now" still waiting for the
spine, picking Fable and closing Preferences writes `rich_model=fable` to `config.json` within
50 ms and puts the toast "Rich now talks with you on Fable." on screen. The CEO's words, the
§119 addendum: *"Closing the preferences panel IS the confirmation that the settings chosen there
now need to be saved and implemented."* This is the real-app half of the fix that runs every
window command under `#[richos_command::command]`, so a blocked command body no longer holds one
of Tauri's async workers (review `rv-20261011T081547Z-c1f0d81a-5017`, P2).

**The app:** an ad-hoc signed development build of `182a3241f`, `1.2.0-dev.182a3241f`
(the guest's `app.log`: `this app: built from 182a3241f325c737aa597d4470f9516dda34ef10`), built
by `richos/app/scripts/package-app.sh`. Engine: nightly 50's `richos-engine-1.2.0.tar.gz`. Home:
an empty folder (a first run). `RICHOS_HOP_TRACE=1` so every spine take and release is logged.
Never on the host's screen (CEO ruling §65).

Run as `TESTVM_APP_ENV=RICHOS_HOP_TRACE=1 run-walk.py --bundle <that zip> --home <empty dir>
--engine <nightly 50 engine> -- steps-walk.py --steps <list>`. The step list and its guest-side
script (steps 12, 14 and 16) ran from the walk's scratch and are not committed: a code file in
the repository needs a suite (`proof-for.sh`), and a one-off check does not earn one. The table
says what each step did.

**Walk `walk-3f62791bed76`**, every step exit 0.

| Step | What the app did |
|---|---|
| 0-8 | first run: "Not now" on the first sheet, company "Acme" with its folder added |
| 9 | "Not now" pressed on the interview card, at once: `decline_onboarding` asks for the spine |
| 10-11 | pointer presses: the gear (407,905) opens Preferences, Fable (423,452) |
| 12 | Escape (173 ms to send); `rich_model` was `opus`, `fable` on disk **50 ms** after Escape. No take of the spine at `src/main.rs:5381` (decline_onboarding) in `app.log` before Escape, nor when Fable landed |
| 12 | frames taken in the guest at +103, +1011 and +2002 ms after the save |
| 13 | the copied-out frame, [`03-toast-after-close.jpg`](03-toast-after-close.jpg): the toast at the top right, the card's "Not now" still there |
| 14 | tesseract on the three guest frames: each reads `Rich now talks with you on Fable.` |
| 15 | `config.json`: `"rich_model": "fable"`, `"stu_model": "opus"` |
| 16 | the decline's take logged **86364 ms after Escape**: `a window command waited 90899 ms for the spine at src/main.rs:5381` |
| 17 | the holder: `spine TAKEN at src/main.rs:1219` (`prime_front_desk`) at +58 ms, `RELEASED ... after holding 98737 ms` at +98796 ms; the decline `TAKEN at src/main.rs:5381 after waiting 90899 ms` at +98797 ms, so it asked at +98797 - 90899 = +7898 ms and waited until +98797 ms, with the save in between |

**Why the toast is read from frames and not the accessibility tree.** A toast lives 3400 ms plus
a 450 ms exit (`settings-button.js` `toast()`). In this guest one accessibility walk of the
window took 5.5 s (83 nodes, read from the host about 1 s after the save) and 6.6 s (84 nodes,
read inside the guest 2 ms after the save): both exhaustive, both "nothing matched" for the
value `Rich now talks with you on Fable`. The walk outlasts the toast, so the frames are taken
in the guest from the moment the save lands and read by their text.

**Cleanup:** run-walk.py's own: "clean: app quit, VM stopped, clone deleted, state removed".
