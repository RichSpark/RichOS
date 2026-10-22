# Failure class: a state the app can find out, shown as "not known yet"

**What it is.** The screen shows a waiting state ("No reading yet", "Not reported yet") for
something the app could establish, so a person cannot tell a slow check from a permanent
problem and the waiting state never ends.

**How it showed (2026-10-21, a new user on 1.2.1).** Settings, Claude accounts: "Your Claude
account · In use now", "No reading yet. If this account is not signed in on this Mac, sign in
here." and both limits "Not reported yet". The card could not say whether the account was
signed in and handed the question to the person.

**Why it could happen.** The card's state was derived only from "has a usage reading"
(`ui/accounts.js` `read(a)`). Signed out, Claude Code answers the usage request exactly as an
account without subscription limits does (`rate_limits_available: false`, measured on 2.1.306),
which the reader stored as `ReadError::Unsupported` and the card never looked at. Yet the app
already had the one call that answers the question: Claude Code's own `claude auth status
--json` (`provider_auth.rs`), used for sign-in and for Show email.

## The rule

When a check comes back without figures, the app asks the question that decides it before it
shows anything: the usage reader asks `claude auth status` for that account's folder and a
signed-out account is `ReadError::SignedOut`, a signed-in one `ReadError::SignedInNoLimits`. Both
verdicts are kept: a signed-in verdict stands on its own and is never lost because no usage
reading came with it. The view carries the verdict
(`quota::View::signed_in` and `AccountView::signed_in` per added account): `Some(false)` signed
out, `Some(true)` once a reading arrived or `auth status` said signed in, `None` while nothing
says either. The card in use that is signed in without limits says so, with no Sign in and no
empty meters. The card then says
"Not signed in", "This account is not signed in on this Mac, so Rich cannot answer you yet. Sign
in here." with Sign in on it and no meters and the Settings row says "Not signed in on this
Mac" in gold. Before the first check has answered it says "Checking whether this account is
signed in on this Mac." and once a check answered something else it says what.

## The checks that guard the class

| Check | What it refuses |
|---|---|
| `quota/probe.rs` `no_limits_from_a_signed_out_account_is_reported_as_signed_out` | a signed-out account read as "no limits" and a signed-in verdict dropped because no limits came with it |
| `ui/tests/accounts.js` "signed in without limits: the card says so, with no Sign in and no empty meters" | Sign in offered to an account Claude Code says is signed in |
| `quota.rs` `the_view_carries_the_sign_in_the_last_check_established` | a view that claims more or less, than the last check established |
| `ui/tests/accounts.js` "signed out: the card says Not signed in with Sign in, the row says it in gold and nothing says No reading yet" | the card or row showing a waiting state for a known sign-out; contrast of both states computed in both themes by the same suite |
| `ui/tests/affordances.js` fixtures `accounts-signed-out`, `accounts-row-signed-out` (rows in `ui/tests/lib/state-registry.js`) | the sentence rendering without its Sign in control |
| `scripts/testvm/new-user-walk.py` dead end `account state unknown` | "No reading yet" on any frame of a real first run |
| `scripts/testvm/new-user-walk.py` `accounts_verdict` (tests in `test/new-user-walk.test.py`) | an accounts step passed on words alone: it needs the Claude accounts sheet open and the card's own Sign in button (`#acc-sign-one`), pressable |

## Other places in the app that belong to this class

| Place | Status |
|---|---|
| `ui/quota.js` Technical quota panel and account lanes | Guarded: the panel distinguishes signed out, signed in without figures and an unknown sign-in. It never asks an account known to be signed in to sign in for want of usage figures. The quota browser suite checks each state and its recovery controls. |
| `ui/accounts.js` an added account with no reading | Unknown is now explicitly unconfirmed for either account, active or idle. A failed check says what failed without claiming sign-out. No empty meters are drawn without a reading. |
| The "outcome not recorded" marks on tool rows | The update and machinery work of the same day; see its class file in this folder. |

## Browser sign-in transitions

Starting or stopping the browser login does not itself establish a new authentication verdict.
`Service::set_connecting` retires the probe and usage figures but preserves the last known
sign-in. Explicit disconnect still clears it. The table test
`every_place_the_sign_in_becomes_known_writes_it_and_a_failed_read_never_erases_it` includes
cancellation and failure through both real connection transitions, rather than merely passing
`Cancelled` to the final setter. `ui/tests/accounts.js` also covers an unknown verdict for
either account, active or idle, before a check and after a failed check, in both themes.

A known sign-in alone does not establish that subscription limits are absent. The card and
Technical view say only that it is signed in until a usage response establishes more.

## A verdict and its message are one fact

`Snapshot::auth_known` updates the boolean, the auth-specific message and obsolete figures
together. An explicit sign-out clears old usage figures; a signed-in answer or streamed
usage reading clears an obsolete signed-out error. Other failed usage reads still leave the
known sign-in standing. The transition table checks messages as well as booleans and refuses
a signed-out account retaining usage figures.

Show email uses `Service::account_email`, which records the sign-in verdict from the same
provider answer and returns the address only for its existing temporary display. No address
or organization identifier enters the quota snapshot. The production-method test
`revealing_an_email_keeps_only_its_auth_verdict_and_never_a_contradictory_message` exercises
both accounts, signed in, signed out and no answer. The browser suite verifies that the
card reflects that verdict immediately instead of contradicting its revealed address.
