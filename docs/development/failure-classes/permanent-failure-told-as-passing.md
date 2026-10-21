# Failure class: a failure that will not clear, told as one that will

**What it is.** The app meets a failure and tells the person it is passing ("this usually fixes
itself", "try again") with a control to try again, when nothing about the failure says it will
clear. When it does not clear, the person is shown the same promise on every attempt: a loop
with no way out.

**How it showed (2026-10-21, a new user on 1.2.1).** Every message ended "RichOS lost its
connection to Claude Code while Rich was answering, so his answer is incomplete. This usually
fixes itself. Press Restore my message, then send it again." Rich never answered once. The
sentence was the `Transient` arm of `interruption.rs`, chosen for any turn whose reason carried
the `cognition io:` prefix, which every mid-turn exit of the Claude Code child did
(`native.rs` `NativeError::Closed`, see
[cause-discarded-at-the-failure-boundary.md](cause-discarded-at-the-failure-boundary.md)).

**Why it could happen.** Two things together:

1. A cause was called passing on the strength of its *shape* (an I/O failure), not of anything
   that showed it would pass.
2. Each failure was judged alone. Nothing looked at the turn before, so the same failure on
   every message got the same "it usually clears" every time.

## The rule

- No sentence the app shows about a failure promises that it clears by itself.
- A failure whose card offers another try cannot follow another such failure in the same
  conversation: the second one is told as repeating (`InterruptionCause::Repeated`), offers no
  retry and names where to look (Settings, Claude accounts, which says whether the account is
  signed in) and how to report it (Bust a bug). An answer in between resets it.
- A permanent cause the app can recognize is classified as itself (not signed in, credential
  refused, Claude Code missing), never as a passing one.

## The checks that guard the class

| Check | What it refuses |
|---|---|
| `crates/richos-core/tests/interruption_tests.rs` `no_cause_promises_that_it_clears_by_itself` | any interruption sentence promising it clears; `Repeated` offering a retry |
| `crates/richos-core/tests/spine_tests.rs` `the_same_lost_connection_again_is_told_as_repeated_and_offers_no_retry` | a retry-offering failure (lost connection or unexplained) following another one; covers every cause whose `offers_retry()` is true, not one arm |
| `crates/richos-core/tests/interruption_tests.rs` `an_oauth_token_anthropic_refuses_is_a_sign_in_to_renew`, `a_child_exit_with_no_sign_in_is_not_signed_in` | a sign-in problem classified as unknown or as a lost connection |
| `crates/richos-core/tests/spine_tests.rs` `a_refused_sign_in_or_key_is_classified_with_its_route_not_taken_for_an_outage` | a refused sign-in or key taken for an outage: both lines carry the vendor's `API Error: 401` and the upstream reader (`spine.rs` `detect_upstream_failure`) claimed them first, so the card named no sign-in route. A cause `interruption::classify` recognizes as somebody's to fix now goes to its own card. |
| `scripts/testvm/new-user-walk.py`, step `signed-out` (two messages), dead end `retry loop on a permanent cause` | the sentence on a real first run, in a guest |
| `nightly-local.py` phase `new-user-walk` (`NEW_USER_WALK_PHASE`) and `require_new_user_walk` at `publish` | a build whose new-user walk did not pass cannot be published |

## Other places in the app that belong to this class

| Place | Status |
|---|---|
| `interruption.rs` `InterruptionCause::Unknown` ("can't tell you why ... send it again") | Guarded: covered by the same repeat rule (`spine.rs` `last_turn_offered_another_try` keys on `offers_retry()`). |
| `interruption.rs` vendor sign-in strings | "OAuth access token is invalid" (Claude Code 2.1.306's 401 for a token Anthropic refuses) now classifies as not signed in; it fell to `Unknown` before. |
| Settings, Updates: "RichOS could not prepare this update." with Try again, for a lock another part of the app held for a moment | Same class, owned by the update work of the same day; see its own class file in this folder. |
| `quota.rs` `ReadError::Failed` "Claude Code did not answer just now." | Not in the class: it backs off and asks again on a schedule and it promises nothing. |
| `ui/dictation.js` "Quitting the app where you last typed a password usually fixes it." | Not in the class: it names the one action that clears it. |

## For new code

A new failure sentence either names the action that clears the cause or says plainly that
the app does not know why. If its card offers another try, it is a cause `offers_retry()`
answers `true` for and the repeat rule then applies to it without any further code.
