# A right-click on a thread row opens the row's own menu (2026-10-13)

## What the CEO showed

His recording of nightly 54, `right-click-in-left-bar.mov` (47.2 s, in the private record), watched with the `watch-video` skill:

- **00:05 to 00:10:** he clicks the `⋯` on the thread row and gets the row's menu: Rename…, Pin, Archive.
- **00:15:** he right-clicks the empty part of the same row and gets WebKit's own one-item "Reload" menu.
- **00:25 to 00:35:** he right-clicks the title. The word is selected, and the system text menu opens (Look Up, Translate, Search with Google, Copy, Share, Speech, Services).
- **00:45:** he goes back to the `⋯` menu.
- **His words:** "I'm expecting when I right-click here anywhere on this row I'm expecting to see the same menu."

## The change

The change is in `ca8b02a50`.

- **The menu:** a right-click anywhere on a thread row now opens the same `openThreadMenu` the `⋯` opens, at the pointer, instead of the browser's menu.
- **The selection:** the row is not text-selectable. WebKit selects the word under a right-click before it sends `contextmenu`, so `preventDefault` alone left the title highlighted.
- **Closing:** a right-click outside every thread row closes the menu.
- **Where it applies:** every rail group builds its rows through `buildThreadRow`, so all of them get the change.

## Test

`richos/app/ui/tests/thread-switch.js` has four new checks: a right-click on the title, on the empty space, on the `⋯`, and a right-click elsewhere that closes the menu.

- On unchanged code, all four fail: the menu never opens.
- With the menu change alone, the title check still fails with "Acme" selected, which reproduces the 00:30 frame.
- With the whole change, the suite passes 13 of 13.

## The real app, in a test VM guest

**The setup:**

- **The bundle:** built from `ca8b02a50` with `package-app.sh`, ad-hoc signed. The guest's `app.log` reads "this app: built from ca8b02a50f4dd84fe3f68c0cefd36cf5bc91ef86".
- **The guest:** fresh, through `run-walk.py`, with an empty home. The app's own first run added the company Acme, and the engine came from nightly 54's tarball.
- **The steps:** `steps/walk.json`, run by `steps-walk.py`. Each click is the real right mouse button (`pointer-drag.sh --right`).

**What each right-click showed:**

| Frame | Right-click at | Seen |
|---|---|---|
| `01-row-before.jpg` | (none) | The thread row "Running" under ACME. |
| `02-right-click-title.jpg` | the title, (195,384) | Rename…, Pin, Archive. The first item sits at (200,389), next to the pointer. The title is not selected. There is no "Reload" and no "Look Up" in the accessibility tree. |
| `03-right-click-empty-space.jpg` | the empty space, (320,384) | The same menu, its first item at (325,389). There is no "Reload". |
| `04-right-click-dots.jpg` | the `⋯`, (413,384) | The same menu, its first item at (418,389). There is no "Reload". |
| `05-after-pin-from-right-click.jpg` | Pin, pressed in that menu | The thread moved under PINNED, so the menu's actions work from this entrance. |

**Afterward:**

- The walk exited 0, and the run ended with "clean: app quit, VM stopped, clone deleted, state removed."
- `ocr-gate.sh` found that 0 of the 5 frames carry anything that must not ship (positive control found).

## Applicability

| Surface | Status |
|---|---|
| Pointer right-click | Changed here. |
| The `⋯` button | Unchanged. |
| Keyboard: the ContextMenu key and Shift-F10 | Unchanged, `main.js` keydown handler. A `contextmenu` event raised from the keyboard, with no pointer position, keeps the anchor under `⋯`. |
| Ways out: Escape, a click, a right-click elsewhere | Escape and a click elsewhere were already there. A right-click elsewhere is added. |
| Light and dark themes | The menu's styles are unchanged and no new color was added, so there is no new contrast surface. |
| The phone apps | Not applicable: right-click is a desktop pointer action, and the phone apps are separate code (`richos/mobile`), not touched here. |
| Voice | Not applicable. |
| Test data | Isolated: a fixture home in a disposable guest. |
