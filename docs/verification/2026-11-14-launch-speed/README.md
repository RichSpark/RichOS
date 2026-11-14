# Launch speed in the test VM, 2026-11-14: v1.3.2 against this branch

Measured with `richos/app/scripts/testvm/launch-speed-walk.py` (run by `run-walk.py --no-app`),
the Dock route (`open -a`, LaunchServices), as a regular launch, macOS 15 guest, 4 processors, on a
host that carried other agents' work at the time (load 7 to 12). Each time is from the launch
request to the page's own splash painted and playing, plus 71 ms for the Dock's own part of a
click and the next screen refresh. The CEO's rule: at most 380 ms p95.

| Route | v1.3.2 (published) median / p95 | this branch median / p95 |
|---|---|---|
| ordinary, 20 launches | 7,197 / 12,389 ms | 2,423 / 3,025 ms |
| first launch after a cold start (cache purged), 3 launches | 10,007 / 12,983 ms | 3,138 / 3,228 ms |
| an older copy in /Applications handing over to the update, 20 launches | 10,406 / 14,477 ms | 2,303 / 2,970 ms |
| **every judged launch, p95 (43 launches)** | **14,477 ms** | **3,138 ms** |
| first launch of a fresh install (reported, not judged) | 86,474 ms | 4,321 ms |

Both are far over 380 ms and the gate refuses both. In the branch's own launch lines the window is
up about 1.0 s after the process starts in this guest and the start-up hook ends about 0.2 s later;
the page then needs about 1.6 s more to paint its splash, the gap Sage's next plan addresses.
v1.3.2 is the published, signed build; the branch's bundle is the same v1.3.2 bundle with this
branch's release executable in it (version 1.3.3), ad-hoc signed.

Start-up checks on the branch (`--only-checks`, after the first run found two harness faults): a
start-up step made to fail (its conversation record unreadable) ends in the start-up alert
(`after-start-up-step-failed-alert.jpg`) and the exit; a quit from the app's own menu during
start-up ends the app (the menu exists only once the start-up hook has returned, so the press
lands then, while the deferred work and the page are still starting); the next launch of the same
home starts. `after-first-launch-1.0s-window-in-splash-ground.jpg` is the screen 1.0 s after a
first launch: the window is on screen in the splash's ground color, never white, before the page.

Every app instance was quit by the pid captured at spawn and checked gone; every guest was stopped
and its clone deleted. Synthetic fixture home only; no one's own app or data.
