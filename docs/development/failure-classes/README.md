# Failure classes

One file per class of defect a user has met: what the class is, the incident that named it,
every other place in the app that belongs to it and the check that guards the whole class.
A new member of a class is found by its check, not by memory.

| Class | Guarded by |
| --- | --- |
| [A busy shared lock reported as a failure](busy-shared-lock-reported-as-a-failure.md) | lint rule `lock-busy-undeclared` (`richos/app/scripts/lint/lock_rules.py`) |
| [Machinery in the user's own view](machinery-in-the-user-view.md) | `Timeline::view` and its tests (`richos-core` `timeline.rs`), `ui/tests/techy.js` checks 1 and 11, `ui/tests/techy-race.js` |
| [Lost input reported without a next step](lost-input-without-a-next-step.md) | `ui/tests/affordances.js`, "every line saying his words did not get through says what to do next" |
