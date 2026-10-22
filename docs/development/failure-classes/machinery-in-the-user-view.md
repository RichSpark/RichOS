# Failure class: machinery in the user's own view

**The class.** The conversation a person reads shows the app's plumbing: protocol frames
(`control_request`, `system:init`, `system:hook_started`), vendor kinds, raw outcomes
("outcome not recorded"), session traffic between turns. Those exist for the Technical view,
which a person turns on to see them. In the ordinary view they read as errors and noise.

**The checks that guard it**, one per layer, because the rows can reach the screen only by
passing all three:

1. **Projection** (`richos-core` `timeline.rs`): `Timeline::view(ViewMode::Ceo)` removes every
   item and between-turn row whose visibility is not `Ceo` and `redacted()` removes the
   technical half (`detail`) of the rest. Vendor kinds, permission requests and unclassified
   tool calls are `Technical` (`machinery_visibility`). Tests:
   `the_incident_frames_never_reach_the_users_own_view` (the exact frames of the incident below,
   in a turn and between turns, with the Technical view as positive control),
   `a_tool_call_that_classifies_to_nothing_is_not_a_worked_row_in_his_view` and the
   `renders_in` table.
2. **Renderer** (`app/ui`): `main.js` `loadTimeline` asks for `get_machinery` only when the
   Technical view is on for that conversation and otherwise empties and hides the "Between
   turns" section; `timeline.js` `visible()` refuses a technical item on a model that is not
   holding a technical snapshot, so even a widened live event cannot draw one. A reload's reply
   is dropped when the view was switched since it asked (`loadTimeline`'s `stale()`); before
   2026-10-21 a reload that asked for the technical view while it was being switched off could
   answer after the switch-off reload and put the rows back with the switch off (review
   rv-20261021T195352Z-f24b36ae-2380). Tests: `ui/tests/techy.js` checks 1 ("off: no technical
   row, no chip, no state line, no hint") and 11 ("on and back off leaves the conversation
   BYTE-IDENTICAL") and `ui/tests/techy-race.js` (the late reply after a switch-off, red
   before that fix).
3. **The real app**: a test-VM walk that reads the screen with the view off. The new-user walk
   (`richos/app/scripts/testvm/new-user-walk.py`, added 2026-10-21) refuses any frame showing
   `control_request`, `system:init`, `outcome not recorded` or `Between turns` (its
   `DEAD_ENDS`). As of 2026-10-21 it has not yet run in a guest or been wired into the nightly's
   gates; until it is, layers 1 and 2 are the standing guard.

## The incident that named it (2026-10-21, RichOS 1.2.1)

A new user's screenshots showed `control_request` and `system:init` rows marked "outcome not
recorded" under "Worked for 3s" and `system:hook_started` / `system:hook_response` under
"Between turns", in a conversation whose first answers had failed.

What the code says about it (1.2.1 is tag `v1.2.1` and this path is unchanged since): those
rows are drawn only from `get_machinery`, which the window requests only when the Technical
view is on for the conversation on screen and the "Between turns" section is filled only from
that payload. With the view off, the projection has already removed them. The screenshots show Settings off at one moment and technical rows later. They do not show
the view being switched on and do not establish who or what changed its state. An earlier
frame lacks the "Between turns" section, but that observation alone cannot reconstruct the
sequence.

The person's `~/Library/Application Support/com.richos.app/config.json` stores the current
global, company and conversation preferences (`techy_default`, `techy_entities` and
`techy_threads`). It is not an event history and does not record every toggle. A
contemporaneous configuration snapshot plus session or timing evidence could help distinguish
a saved preference from a rendering race. A current file alone cannot settle the incident.

Review found a real path that puts rows on screen with the switch off: a reload that asked
for the Technical view can return after it has been switched off and overwrite the plain
view. The stale-response check fixes that path, with a regression that failed before the
fix. Whether that race caused the historical screenshots remains unconfirmed without the
incident evidence.

## Other places that could show machinery, as of 2026-10-21

| Surface | Why it does not |
| --- | --- |
| Live events (`rich://activity-upserted` and the rest of the calm family) | `LiveEvent::may_reach_webview` admits only `Visibility::Ceo`; `timeline.js` `visible()` refuses the rest |
| The phone (`src-tauri/src/phone/rows.rs`) | `event_from_live` drops activity, worker and turn-status events entirely |
| Cached conversations (`main.js` `threadViews`) | the cache stores the techy answer with the model and is cleared whenever the view is switched |
| The Output panel's "Between turns" group (`output-panel.js`) | it lists files Rich produced between turns, not session traffic |
| Bug reports (`bug-report.js`) | record only whether the view was on, never machinery content |
