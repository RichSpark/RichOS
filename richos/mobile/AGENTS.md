# Mobile development

Judge every user-visible state by the CEO's experience, not just technical success.
Healthy operation and routine recovery must be seamless and invisible: do not announce
normal connectivity or require the user to manage retries. Preserve the conversation,
drafts and recordings. Show only meaningful persistent interruptions or necessary
actions, based on evidence. Protect these journeys with durable regression tests.

Every mobile feature, fix and refactor must preserve this development loop:

- Put application actions and semantic state in `core/`, without DOM or native UI dependencies. UI code invokes the same actions as the CLI.
- Reuse the reference client modules in `richos/web/web-app/lib` where applicable. Do not make a second queue, retry policy or test-only implementation of application behavior.
- Inject storage, transport, clock and platform operations. Extend deterministic scenarios when behavior changes. Add recording and update-policy scenarios when those features are implemented, using the real core with replaceable platform results.
- Run the focused headless checks first. Use the same CLI to enter simulator states directly when UI or native behavior matters. Verify visible controls through UI automation; direct core calls do not prove taps work.
- Keep fixtures and external development commands out of Release builds. Run `check-release` when changing the native bridge or packaging.
- Record actual feedback times privately in `richos-hq`. Keep build output and generated projects outside source on the external SSD. Follow the user's simulator-storage choice rather than silently changing system storage.
- The phone web page (PWA) was retired on 2026-10-30; do not bring it back. The current Swift/WKWebView shell is the minimal development host; it does not settle the final mobile framework or prove real-device microphone, APNs or managed networking behavior.
- Shared behavior belongs in reusable headless modules; platform differences need explicit checks, not a second implementation of shared rules.

See `DEVELOPMENT.md` for commands and the verification boundaries.

## Battery: the answer is NO before a commit (CEO ruling §81)

Priority #2 for all mobile development, after super-fast CLI development above (§76). The CEO's words, 2026-09-24 and they are the rule: *"Here's my #2 priority for all mobile development: This is only for work that touches any files in the mobile folder. Before committing a set of changes or before declaring the work as finished etc, the worker must always answer this question: Would this change/set of changes get the mobile OS (regardless of whether its iOS or Android) to complain that RichOS is a "power-intensive app" that's "draining battery by frequently refreshing in the background" or anything similar? And if the answer is anything other than a clear NO, then the changes must be revised until the answer becomes a resounding NO. The RichOS app must NEVER do or cause anything that might annoy the user."*

Every commit that touches `richos/mobile/` ends with this trailer, evidence included:

```
Battery-check: NO — <what you checked, and what it measured>
```

Write it on ONE line or indent every continuation line by at least one space; git reads an unindented wrapped line as plain text and finds no trailer at all (2026-10-01). Check with `git log -1 --format=%B | git interpret-trailers --parse`: it must print the trailer. The commit itself is refused when it does not parse (autocheck's `commit-msg` hook) and the land refuses a commit without it or with anything but `NO` and evidence (`richos/app/scripts/battery-check.py`, run by `proof-for.sh` and `proof-run.py`). Check your branch before handing off with `python3 richos/app/scripts/battery-check.py` and repeat the answer in the handoff. The trailer proves you answered; the evidence is what makes the answer true.

The evidence covers what your change adds, changes or removes on each platform it touches:

- **Background work and its schedule.** Useful bounded work may run without background interaction, including preparation that measurably improves opening or foreground return. Every such operation must have enforced duration, frequency, retry and cumulative resource limits and measured benefit. No indefinite stream, reconnect loop, outbox timer or keep-alive stays open in the background; push carries awareness, the foreground reconciles. The user is expected to record voice messages with the app visible. Unexpected backgrounding preserves captured audio without sending and releases the microphone.
- **Wakeups and timers.** No repeating timer, alarm or coroutine loop ticks while backgrounded or idle. A timer exists only while its work is due (a recording running, a retry owed).
- **Idle redraws.** An idle screen renders 0 frames per second. The only exception is a bounded, short animation that stops by itself. Nothing repeats forever (`rememberInfiniteTransition`, `TimelineView(.animation)`, `repeatForever`) unless the user is watching live work.
- **Network.** Push, never polling. Retries back off, stop when the cause is permanent (revoked, incompatible) and wait for the network or the foreground.
- **Wake locks and foreground services.** None, unless the user is actively recording or playing audio and released the moment that ends.
- **Push.** Only as often as there is something the person should know. The notification service extension does bounded work and makes no extra network round trips per notification.
- **Location, Bluetooth, camera, microphone.** Only while the user is using them and stopped on leaving the screen.

Where each OS gets its "power-intensive" judgment (read 2026-09-24):

- **Android.** [Android vitals](https://developer.android.com/topic/performance/vitals) tracks excessive partial wake locks (a core vital: [2 or more hours in 24, in over 5% of sessions over 28 days](https://developer.android.com/google/play/vitals/excessive-wakelock); Play may reduce visibility and warn on the listing), stuck partial wake locks, [excessive wakeups](https://developer.android.com/topic/performance/vitals/wakeup) (use WorkManager, not repeating `AlarmManager` alarms), background Wi-Fi scans and background network use. [Doze and App Standby](https://developer.android.com/training/monitoring-device-state/doze-standby) defer network, jobs and alarms and ignore wake locks; high-priority FCM is only for messages that show a notification. Phone makers' own battery managers flag the same behaviors: the "Power-intensive app found … draining battery by frequently refreshing in the background" notice that prompted this rule is one.
- **iOS.** Settings > Battery shows each app's foreground and background use and a significant 24-hour use produces an energy exception report ([Analyzing your app's battery use](https://developer.apple.com/documentation/xcode/analyzing-your-app-s-battery-use): Xcode Organizer Battery Usage and Energy panes, the Energy Impact gauge, MetricKit). Background runtime comes only through the system's schedulers ([Choosing background strategies](https://developer.apple.com/documentation/backgroundtasks/choosing-background-strategies-for-your-app): `BGAppRefreshTask`, `BGProcessingTask`, `beginBackgroundTask`) and background pushes are throttled: [send no more than two or three per hour](https://developer.apple.com/documentation/usernotifications/pushing-background-updates-to-your-app).
