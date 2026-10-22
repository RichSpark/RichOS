# Failure classes

One file per class of defect a user has met: what the class is, the incident that named it,
every other place in the app that belongs to it and the check that guards the whole class.
A new member of a class is found by its check, not by memory.

| Class | Guarded by |
| --- | --- |
| [A busy shared lock reported as a failure](busy-shared-lock-reported-as-a-failure.md) | lint rule `lock-busy-undeclared` (`richos/app/scripts/lint/lock_rules.py`) |
| [Machinery in the user's own view](machinery-in-the-user-view.md) | `Timeline::view` and its tests (`richos-core` `timeline.rs`), `ui/tests/techy.js` checks 1 and 11, `ui/tests/techy-race.js` |
| [Lost input reported without a next step](lost-input-without-a-next-step.md) | `ui/tests/affordances.js`, "every line saying his words did not get through says what to do next" |
| [Permanent failure told as passing](permanent-failure-told-as-passing.md) | interruption classification and consecutive-failure tests, first-run walk |
| [Cause discarded at the failure boundary](cause-discarded-at-the-failure-boundary.md) | native child-exit and usage-reader crash/recovery tests |
| [Machine words shown as Rich's words](machine-words-as-richs-words.md) | native API-error filtering and first-run walk |
| [Unknown state shown as pending](unknown-state-shown-as-pending.md) | authentication transition tests, account rendering and first-run walk |
| [First run never tested as a customer](first-run-never-tested-as-a-customer.md) | new-user walk and candidate publication gate |

These checks have explicit scopes. A declaration lint checks that a reviewer is told how busy
is handled; behavior tests verify the relevant implementation. A phrase scan cannot establish
the meaning of arbitrary future prose. Guest credential projection does not prove completing
external browser authentication. Keep those boundaries with the evidence instead of treating
an instruction or a passing check as a universal guarantee.
