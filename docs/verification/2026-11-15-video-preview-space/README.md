# Space on the "Make a video" sheet's player, on the real app (2026-11-15)

The handover review of `cc/echo4-fable-videobtn1` at `b429f9524`
(`rv-20261115T003622Z-b429f952-3184`) found that Space on the focused preview opened Talk to Rich
instead of pausing the video. Fixed in `a55daed50` (`richos/app/ui/home-video.js`, the
capture-phase key handler).

The app built from `a55daed50` (`branch-bundle.sh`, engine 1.2.0 pinned as v1.3.2 pins it) ran in
a fresh test VM guest started by `run-walk.py --no-audio` (no sound device) and was driven by
`richos/app/scripts/testvm/home-video-walk.py --steps seed,space`. Nothing ran on the host's
screen and nothing played sound: the app was started with `RICHOS_VIDEO_INPUT_TONE=1` and the
sheet's player is muted for it.

| Reading | Controls offer | Timeline (fraction) 1.5 s apart | Home screen | Sheet |
|---|---|---|---|---|
| Player focused, before Space | Pause | 0 then 0.641 (moving) | up | up |
| After Space | Play | 0.915 then 0.915 (standing) | up | up |
| After Space again | Pause | 0.194 then 0.856 (moving) | up | up |

`13-space-paused.jpg` is the guest's screen after the first Space: the play glyph, the focus ring
on the player, 0:05 of 0:06 and the sheet over the home screen. Every reading is in
`walk-report.json.txt`. `run-walk.py` quit the app by its pid (2174), stopped the VM and deleted
the clone.
