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
| `ui/quota.js` "No reading yet." (Technical view's detailed quota row) | Open. Technical view only; the account card everyone sees is fixed. The same `signedIn` field is in the view it reads. |
| `ui/accounts.js` an added account with no reading is drawn "Not signed in" | The opposite error (asserting signed out without knowing). Open: the view now carries `signedIn` per added account, so the card can tell the two apart; that card is round 18's chosen design and is changed only with a design round. |
| The "outcome not recorded" marks on tool rows | The update and machinery work of the same day; see its class file in this folder. |
