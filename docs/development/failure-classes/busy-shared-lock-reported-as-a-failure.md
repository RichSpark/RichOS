# Failure class: a busy shared lock reported as a failure

**The class.** Two parts of RichOS (two threads, two windows, two copies of the app or the
app and its own startup) share a file lock. One of them takes it for a moment. The other tries
it once, without waiting, finds it held and treats "busy" as "broken": the user sees a failure
headline, often with the operating system's words behind it ("Resource temporarily unavailable
(os error 35)"), for something that would have succeeded a few milliseconds later. A close
relative is the same lock taken twice on one thread, which can never succeed and, with a wait,
costs the whole wait before failing.

**The check that guards it:** the lint rule `lock-busy-undeclared`
(`richos/app/scripts/lint/lock_rules.py`, wired in `richos/app/scripts/lint/driver.py`, ceiling
0 in `richos/app/scripts/lint/baselines/custom.json`, unit tests in
`richos/app/scripts/lint/test_lock.py`). Every non-blocking lock attempt (`LOCK_NB`) in the
app's Rust must carry, on its line or one of the three above it,

```rust
// lock-busy: <what the caller does when the lock is busy>
```

A bare marker or a reason under ten characters declares nothing. The rule runs in every commit
check (`lint.sh --changed --strict`) and in the land check (`lint.test.sh`), so a new try-once
lock cannot reach `main` without someone writing down, where a reviewer sees it, either the
bounded wait it sits in or why "busy" is the answer its caller wants.

## The incident that named it (2026-10-21, RichOS 1.2.1)

A new user opened Settings > Updates and read "RichOS could not prepare this update." with the
technical reason "Resource temporarily unavailable (os error 35)". Nothing was being prepared:
1.2.1 was the newest release, so the check never downloaded anything.

- `updates.rs` `check()` reads the update record with `richos_user_update::staged()` and every
  error from it became the "could not prepare" headline.
- `staged()`, `last_activation()`, `rollback_target()` and `collect_retired()` took the update
  record's lock (`~/Applications/.richos-updater/lock`) with one `flock(LOCK_EX | LOCK_NB)` and
  no wait. `EWOULDBLOCK` is errno 35 on macOS.
- The app has more than one reader of that record at the same time in normal use: the launch
  check (`spawn_launch_check`) and the window's own `update_state` (which reads
  `rollback_target()`), on different threads. Whichever came second failed.
- The same lock was also taken a second time, on the same thread, inside `with_preferred`'s
  callback in `update_startup.rs` (`last_activation()`), where it could never succeed: the
  rollback-redirect record it read was always treated as absent.

The fix: `Lock::acquire` (richos-user-update `lib.rs`) waits up to `PUBLICATION_WAIT` (5 s) for
another holder and has no try-once form; a take on the thread that already holds the lock is
refused at once instead of waiting for itself; a lock still busy after the wait says so in
words (`PUBLICATION_BUSY`, kind `WouldBlock`); and `updates.rs` shows a busy record as "RichOS
is busy with another update task. Try again in a minute." (kind `busy`), never as a failure to
prepare. `update_startup.rs` reads `last_activation()` before taking the lock. Tests:
`journal_readers_wait_for_a_short_hold_instead_of_failing`,
`a_second_take_on_the_holding_thread_is_refused_at_once_not_after_the_wait`,
`a_lock_still_busy_after_the_wait_says_so_in_words` (richos-user-update) and
`a_busy_update_record_is_busy_and_never_a_failure_to_prepare` (src-tauri `updates.rs`).

## Every other member of the class in the app, as of 2026-10-21

Each site now carries its `lock-busy:` declaration; this is what they say.

| Site | What busy means there |
| --- | --- |
| richos-user-update `Lock::named` (`lock`, `stage.lock`) | one try; `acquire` polls it for 5 s; a busy `stage.lock` reaches `updates.rs` as busy |
| richos-user-update `StartupLease::acquire` (exclusive probe) | another session runs; take the shared side, do not activate |
| richos-user-update `StartupLease::acquire` / `adopt` (shared side) | a startup holds the exclusive side; the caller's `wait_for_lock` retries for 10 s, a staging says busy |
| richos-user-update `begin_session` | conversion of a lease this process holds; the caller retries |
| richos-user-update `wait_for_sessions_to_end` | one poll inside a bounded wait |
| richos-core `app_workers.rs` evidence lock | one poll inside 500 ms; busy past it is "evidence unavailable", an unknown state |
| richos-core `operator_claim.rs` claim lock | one poll inside the caller's wait; then a sentence saying it stayed busy |
| richos-core `quota/holds.rs` pause record | busy is the answer: a liveness probe |
| richos-core `quota/resets.rs` reset record | held across a network poll, so answered at once in words ("A reset check or action is in progress. Try again shortly.") |
| src-tauri `dictation/ipc.rs` key socket | busy is the answer: another process owns the socket |

## What the check does not see

`Mutex::try_lock` and other in-process primitives are outside it. Each use read on 2026-10-21
treats busy as a state, not a failure: the playout callback skips a frame rather than block
audio, the spine and lease probes (`spine.rs`, `work_host.rs`, `quota.rs`) move on and the
update work gate (`updates.rs`) reads a busy conversation as "unknown, so wait". None of them
reports busy to the user as an error. A file lock taken through anything other than
`libc::flock` would not contain `LOCK_NB`: product code has none today (the standard library's
`File::try_lock` appears only in `richos-core/tests/questions.rs`, as a probe). If one is
added, extend `lock_rules.SITE` to see it.
