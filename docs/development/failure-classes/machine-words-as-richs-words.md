# Failure class: a machine's words shown as Rich's words

**What it is.** Text that a program wrote for a developer (a vendor error line, a terminal
instruction) reaches the conversation as if Rich had said it, so a person reads an instruction
they cannot follow, in Rich's voice and the app counts it as part of his answer.

**How it showed (2026-10-21, a guest walk of 1.2.1 as a new user, signed out).** The first
message ended with "Rich: Not logged in · Please run /login" above the app's own sign-in card,
and the card said "the 33 characters of the answer that had already arrived are saved": the
vendor line is exactly 33 characters. A person using the app has no terminal to run `/login` in.

**Why it could happen.** Claude Code reports an API failure as an `assistant` message marked
`is_api_error_message: true`, then a `result` with `is_error: true`. The reader already held that
line back until the turn's end so it would not become a continued turn's reply (the connection
work of 2026-10-21), but it still delivered it as the reply's text whenever the turn ended. The
timeline's own filter (`upstream::split_at_vendor_diagnostic`) only demotes text after the
vendor's `API Error:` prefix and this line has none.

## The rule

A turn that FAILS on Claude Code's API error never gets that line as Rich's reply: its card
says what happened in the app's own words and the line stays in the turn's recorded reason for
Technical view (`native.rs` `end_turn`). The decision is made from Claude Code's own structure
(`is_error`), never from the wording.

## The checks that guard the class

| Check | What it refuses |
|---|---|
| `native.rs` `native_driver_tests::an_api_failure_while_the_connection_is_up_ends_the_turn_as_before` (the signed-out line and the Cloudflare 403) | the vendor's line reaching the reply of a failed turn |
| `scripts/testvm/new-user-walk.py`: dead end `machine words as Rich's words` ("Please run /login") and `signed_out_verdict` failing a failed turn that recorded any text as Rich's | the same on a real first run |

## Other places in the app that belong to this class

| Place | Status |
|---|---|
| `timeline.rs` / `upstream.rs` `split_at_vendor_diagnostic`: text after `API Error:` in a run | Guarded since 2026-09-05; the filter for text that did reach a run. |
| `native.rs` `overloaded_api_error_message`: an overload reported as a message in a turn that still ends | Guarded: not passed off as the reply. |
| Machinery rows (`control_request`, `system:init`, "outcome not recorded") in the conversation | Same family, owned by the machinery-rows work of the same day; see its class file in this folder. |
