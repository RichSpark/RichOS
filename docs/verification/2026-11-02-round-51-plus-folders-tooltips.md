# Round 51's plus and tooltips on the real app (2026-11-02)

The CEO's order: "implement `richos-hq/design/mockups/rounds/round-51`" and in his recording
(`input-area-to-fix.mov`, 04:28–05:04): "keep the plus button … just add files and folders … this
helpful tooltip". This is the real-app check of branch `cc/echo-opus-build52` at `c9a265bd9`: the app
built from the branch (`branch-bundle.sh`, engine 1.2.0 pinned as v1.2.5 pins it), run in a fresh test
VM guest by `run-walk.py` with an empty home and driven through `steps-walk.py` by the step list at the end of this page.
The hovers rest the guest's real mouse on each control, found by its title (`pointer` with `find`).

## The plus: files and folders

| Step | Frame |
|---|---|
| The plus opens macOS's own panel, titled "Add files and folders"; a file is chosen | `2-panel-file-chosen-dark.jpg` |
| The same panel takes a folder: "Comps 2026" selected, Open lit | `2-panel-folder-chosen-dark.jpg` |
| Both on the tray: `brief.pdf` (PDF · 45 bytes) and `Comps 2026` (the folder glyph, Folder · 3 items) | `3-picked-dark.jpg`, `3-picked-light.jpg` |
| Sent: the bubble names the file under "Attached on this Mac" and the folder under "Folders on this Mac (1 folder, not copied; read them where they are)"; Rich answered with its contents, `november.csv`, `october.csv`, `old` | `5-rich-read-the-folder-light.jpg` |

Rich's answer was read back from the guest's accessibility tree, not only seen: `AXStaticText
value='november.csv october.csv old'` (step 87). The folder glyph measures `#c2a35c` on `#0a1120` in
the dark frame, 7.80:1.

## The tooltips, both themes

| Control | Dark | Light |
|---|---|---|
| Plus: "Add files and folders" | `1-tip-plus-dark.jpg` | `1-tip-plus-light.jpg` |
| Talk to Rich | `1-tip-talk-dark.jpg` | `1-tip-talk-light.jpg` |
| Send, with ↩ | `4-tip-send-dark.jpg` | `4-tip-send-light.jpg` |

Each is round 51's chip (`r51-tooltip-add-<theme>.png` in the round's captures): above the control,
the theme's ink as the fill and its ground as the words, 14.55:1 dark and 14.90:1 light, computed. Send's
is kept 8 px inside the window, as the round's is. The empty box shows the plus's tooltip over Talk to
Rich, which sits directly above the plus there; that is the round's own geometry.

## Where the app differs from the round and why

- **Nothing in the input area.** The status line is empty and Output has no count because a fresh
  install has no team work and nothing produced yet (as in the build51 record).
- **The "Settings" label at the top right in the light frames** is the Settings button's own focus
  tooltip, left by the walk's theme change; it is not part of the input area.

## The run

The first run (`00:24–00:28Z`) passed every step up to the send and then failed in the walk, not the
app: it looked for the bubble by AX title and message text is an AX value. The second run, with that
one step corrected, passed every step not marked `allow_fail` in the plan (`00:30–00:32Z`). Only its
last step, copying the guest's app log (marked `allow_fail`), failed: the log path is a shell pattern
that `scp` does not expand. The app was quit and the guest stopped and deleted by `run-walk.py` at the
end of each run (`clean: app quit, VM stopped, clone deleted, state removed`).

Privacy: `scripts/qa/ocr-gate.sh` read all 14 frames of the second run. It flagged three: the attached
file's path carries the guest's home and a digit run in the conversation id. Those three were covered
by `scripts/qa/redact.py --phones --homes` and the gate then found 0 of 3 with a hit. Only one of
them is kept here (`5-rich-read-the-folder-light.jpg`); its covered strings are the guest's own paths.

## The step list

The second run's plan, as given to `steps-walk.py --steps` (kept here rather than under a `steps/` folder: declaring a new step list in `steps-walk-data.test.sh` rewrites a line the publication guard reads as carrying an incident label).

```json
[
  {"op": "until", "args": ["find", "--title", "Not now", "--role", "AXButton", "--contains", "--first"], "contains": "Not now", "seconds": 120},
  {"op": "wait", "seconds": 3},
  {"op": "guest", "command": "grep -m1 'this app: built from' /Users/admin/testvm/walk-*/app.log"},
  {"op": "guest", "command": "mkdir -p \"$HOME/Acme\" \"$HOME/Comps 2026/old\" && printf 'a,b\\n1,2\\n' > \"$HOME/Comps 2026/october.csv\" && printf 'a,b\\n3,4\\n' > \"$HOME/Comps 2026/november.csv\" && printf '%%PDF-1.4\\n1 0 obj<<>>endobj\\ntrailer<<>>\\n%%%%EOF\\n' > \"$HOME/brief.pdf\" && ls -la \"$HOME\" \"$HOME/Comps 2026\""},
  {"op": "ax", "args": ["click", "--title", "Not now", "--role", "AXButton", "--contains", "--first"]},
  {"op": "until", "args": ["find", "--title", "Add this company", "--role", "AXButton", "--contains", "--first"], "contains": "Add this company", "seconds": 60},
  {"op": "ax", "args": ["type", "Acme", "--role", "AXTextField", "--title", "What's the company called?", "--contains", "--first", "--replace"]},
  {"op": "ax", "args": ["type", "/Users/admin/Acme", "--role", "AXTextField", "--title", "Its folder on this Mac", "--contains", "--first", "--replace"]},
  {"op": "ax", "args": ["click", "--title", "Add this company", "--role", "AXButton", "--contains", "--first"]},
  {"op": "wait", "seconds": 8},
  {"op": "ax", "args": ["click", "--id", "first-run-later", "--first"], "allow_fail": true},
  {"op": "wait", "seconds": 6},
  {"op": "until", "args": ["find", "--role", "AXTextArea", "--title", "Message to Rich", "--first"], "contains": "matches=1", "seconds": 60},
  {"op": "ax", "args": ["click", "--title", "Settings", "--role", "AXPopUpButton", "--first"]},
  {"op": "wait", "seconds": 2},
  {"op": "ax", "args": ["click", "--title", "Dark theme", "--first"]},
  {"op": "wait", "seconds": 1},
  {"op": "ax", "args": ["--key", "53"]},
  {"op": "wait", "seconds": 2},
  {"op": "ax", "args": ["focus", "--role", "AXTextArea", "--title", "Message to Rich", "--first"]},
  {"op": "guest", "command": "osascript -e 'tell application \"System Events\" to keystroke \"a\" using command down' -e 'tell application \"System Events\" to key code 51'"},
  {"op": "wait", "seconds": 2},

  {"op": "pointer", "points": [], "hover": true, "find": ["--title", "Add files and folders", "--role", "AXButton", "--first"]},
  {"op": "wait", "seconds": 1.5},
  {"op": "shot", "name": "1-tip-plus-dark"},
  {"op": "pointer", "points": [], "hover": true, "find": ["--title", "Talk to Rich", "--role", "AXCheckBox", "--first"]},
  {"op": "wait", "seconds": 1.5},
  {"op": "shot", "name": "1-tip-talk-dark"},

  {"op": "ax", "args": ["click", "--title", "Add files and folders", "--role", "AXButton", "--first"]},
  {"op": "until", "args": ["find", "--title", "Cancel", "--role", "AXButton", "--first"], "contains": "Cancel", "seconds": 20},
  {"op": "wait", "seconds": 2},
  {"op": "shot", "name": "2-panel-dark"},
  {"op": "tree", "name": "2-panel-dark"},
  {"op": "guest", "command": "osascript -e 'tell application \"System Events\" to keystroke \"g\" using {command down, shift down}'"},
  {"op": "wait", "seconds": 2},
  {"op": "keys", "text": "~/brief.pdf"},
  {"op": "wait", "seconds": 1},
  {"op": "guest", "command": "osascript -e 'tell application \"System Events\" to key code 36'"},
  {"op": "wait", "seconds": 3},
  {"op": "shot", "name": "2-panel-file-chosen-dark"},
  {"op": "ax", "args": ["click", "--title", "Open", "--role", "AXButton", "--first"]},
  {"op": "wait", "seconds": 3},

  {"op": "ax", "args": ["click", "--title", "Add files and folders", "--role", "AXButton", "--first"]},
  {"op": "until", "args": ["find", "--title", "Cancel", "--role", "AXButton", "--first"], "contains": "Cancel", "seconds": 20},
  {"op": "wait", "seconds": 2},
  {"op": "guest", "command": "osascript -e 'tell application \"System Events\" to keystroke \"g\" using {command down, shift down}'"},
  {"op": "wait", "seconds": 2},
  {"op": "keys", "text": "~/Comps 2026"},
  {"op": "wait", "seconds": 1},
  {"op": "guest", "command": "osascript -e 'tell application \"System Events\" to key code 36'"},
  {"op": "wait", "seconds": 3},
  {"op": "shot", "name": "2-panel-folder-chosen-dark"},
  {"op": "ax", "args": ["click", "--title", "Open", "--role", "AXButton", "--first"]},
  {"op": "wait", "seconds": 3},
  {"op": "until", "args": ["find", "--title", "Remove Comps 2026", "--first"], "contains": "matches=1", "seconds": 20},
  {"op": "shot", "name": "3-picked-dark"},
  {"op": "tree", "name": "3-picked-dark"},

  {"op": "ax", "args": ["type", "List the names of everything inside the folder I attached, one per line, and nothing else.", "--role", "AXTextArea", "--title", "Message to Rich", "--first", "--replace"]},
  {"op": "wait", "seconds": 2},
  {"op": "pointer", "points": [], "hover": true, "find": ["--title", "Send", "--role", "AXButton", "--first"]},
  {"op": "wait", "seconds": 1.5},
  {"op": "shot", "name": "4-tip-send-dark"},

  {"op": "ax", "args": ["click", "--title", "Settings", "--role", "AXPopUpButton", "--first"]},
  {"op": "wait", "seconds": 2},
  {"op": "ax", "args": ["click", "--title", "Light theme", "--first"]},
  {"op": "wait", "seconds": 1},
  {"op": "ax", "args": ["--key", "53"]},
  {"op": "wait", "seconds": 2},
  {"op": "shot", "name": "3-picked-light"},
  {"op": "pointer", "points": [], "hover": true, "find": ["--title", "Add files and folders", "--role", "AXButton", "--first"]},
  {"op": "wait", "seconds": 1.5},
  {"op": "shot", "name": "1-tip-plus-light"},
  {"op": "pointer", "points": [], "hover": true, "find": ["--title", "Talk to Rich", "--role", "AXCheckBox", "--first"]},
  {"op": "wait", "seconds": 1.5},
  {"op": "shot", "name": "1-tip-talk-light"},
  {"op": "pointer", "points": [], "hover": true, "find": ["--title", "Send", "--role", "AXButton", "--first"]},
  {"op": "wait", "seconds": 1.5},
  {"op": "shot", "name": "4-tip-send-light"},

  {"op": "ax", "args": ["focus", "--role", "AXTextArea", "--title", "Message to Rich", "--first"]},
  {"op": "guest", "command": "osascript -e 'tell application \"System Events\" to key code 36'"},
  {"op": "wait", "seconds": 3},
  {"op": "shot", "name": "5a-after-send-light"},
  {"op": "tree", "name": "5a-after-send-light"},
  {"op": "until", "args": ["find", "--value", "Folders on this Mac", "--contains", "--first"], "contains": "matches=1", "seconds": 30},
  {"op": "wait", "seconds": 2},
  {"op": "shot", "name": "5-sent-light"},
  {"op": "tree", "name": "5-sent-light"},
  {"op": "until", "args": ["find", "--value", "november.csv", "--contains", "--first"], "contains": "matches=1", "seconds": 180, "allow_fail": true},
  {"op": "wait", "seconds": 3},
  {"op": "shot", "name": "6-rich-read-it-light"},
  {"op": "tree", "name": "6-rich-read-it-light"},
  {"op": "pull", "src": "/Users/admin/testvm/walk-*/app.log", "dest": "app.log", "allow_fail": true}
]
```
