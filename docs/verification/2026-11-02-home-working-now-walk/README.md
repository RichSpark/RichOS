# The home screen's "Working now" column on the real app (2026-11-02)

The CEO's rule: only the teammates working now are listed; more than round 11.1's column holds, the
list is cut off; nobody working, no column; the full team never on the home screen.

Real-app check of branch `cc/echo-sonnet-home1` at `da872f263`: the app built from the branch
(`testvm/branch-bundle.sh`, engine 1.2.0 pinned as the v1.2.5 release pins it, sha256 `80d40c80…4f9e30`),
run once in a fresh test VM guest by `run-walk.py` with an empty home, driven by `steps-walk.py` with
`steps/walk.json.txt`. The guest's `claude` is `fake-claude-fill-first.pl`, which journals the provider's
SubagentStart and async-launched rows for the names in `agents` (the same rows `app_workers.rs` reads); a
team of 35 is written by `testvm/seed-team.sh` (headings in all three separator forms). Fictional data only.
All 83 steps exited 0 (`steps.json.txt`); `clean: app quit, VM stopped, clone deleted, state removed`.

| State | Home screen | Token spend screen (same count) | Round 11.1 |
|---|---|---|---|
| 35 on the team, nobody working: no column, no heading, none of the 35 names | `1-team-of-35-nobody-working.jpg` | "No agents working right now", `1b-token-spend-none.jpg` | `round-11.1-v1-reference.jpg` |
| Two working (Sage, Will), each with role and gold dot | `2-two-working.jpg` | "2 agents working", `2b-token-spend-two.jpg` | same |
| Nine working: the first six listed (Finn, Gavin, Grant, Lance, Sage, Beth), no "more" line | `3-nine-working-cut-off-at-six.jpg` | "9 agents working", `3b-token-spend-nine.jpg` | same |

Finn, Gavin, Grant, Lance and Sage have headings with ":" or " - " and show with their roles.

## Found on the way

With one company there is no entity row and no banner, so the column's heading sat at y=32 under the
settings button ("WORKING NO" cut off). Fixed in `home.css` (floor of 66px as the banner's); the first run
of this walk with the unfixed CSS showed it.

## Not claimed

- No relaunch/crash case for the working list beyond the three states. Light theme: the home screen is
  clamped dark in both themes (§15).
- A user with no teammates still sees the first-run demonstration's illustrative "Working now" column under
  its banner (CEO 2026-09-06: "The demo is definitely needed, initially, for the user"); left as is.
- The walk's plugin-qualified type (`richos-app-engine:<name>`) is covered by the Rust unit test, not the walk
  (the fake journals bare names).
- Earlier attempts: two runs stopped in `run.sh` before the app started (a 480 s guest boot timeout,
  twice) and three failed on walk steps I corrected; the final run is the one recorded here.
