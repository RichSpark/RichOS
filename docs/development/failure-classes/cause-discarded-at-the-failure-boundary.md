# Failure class: the cause is thrown away where the failure is caught

**What it is.** At the moment something fails, the app holds (or could ask for) the reason and
the code that turns the failure into an error keeps only a fixed sentence. Everything
downstream (the card the person sees, the ledger, a bug report) can then only guess and nobody
can find out afterwards what happened on that Mac.

**How it showed (2026-10-21, a new user on 1.2.1).** The Claude Code child ended in the middle
of every turn, after `system:init` and before any `result`. The reader thread saw stdout close
and ended the turn with the fixed frame `{"stop_reason":"child_exited"}`; `native_result` turned
that into `NativeError::Closed`, whose whole text is "claude channel closed (child exited?)". The
child's own stderr, which the lease was already keeping in `stderr_tail`, was dropped on that
path. So the turn's recorded reason said nothing about why, the card could only call it a lost
connection and nothing on the Mac recorded the cause.

**Why it could happen.** The startup path had been taught to quote the child's stderr
(`NativeError::Startup`), but the mid-turn path was a separate place that ended turns and it was
written before anyone needed its reason.

## The rule

A failure carries what the failing party said, at the point it is caught:

- A child that ends mid-turn ends the turn with `NativeError::Exited { said }`: its last stderr
  lines (waited for up to 500 ms after stdout closes) or "it wrote nothing on stderr" and, when
  Claude Code's own `claude auth status` then says nobody is signed in to the account the lease
  ran under, `CHILD_EXIT_SIGNED_OUT`. The turn's reason, the classifier and the ledger all see it.
- `interruption::classify` reads that reason, so the child's own words decide the cause when they
  name one (a sign-in, a refused key) and a sign-out Claude Code reports is told as a sign-in.

## The checks that guard the class

| Check | What it refuses |
|---|---|
| `native.rs` `native_driver_tests::a_child_that_ends_mid_turn_names_why_in_the_turns_reason` | a mid-turn exit whose reason lacks the child's stderr or the sign-in verdict |
| `native.rs` `native_driver_tests::child_exit_said_reports_an_empty_stderr_and_no_sign_in_verdict_plainly` | an empty or overlong tail said wrongly |
| `interruption_tests.rs` `a_child_exit_with_no_sign_in_is_not_signed_in` | the carried cause not being read by the classifier |

## Other places in the app that belong to this class

| Place | Status |
|---|---|
| `native.rs` `NativeError::Startup` (handshake failures) | Already right: quotes the child's stderr. The model for the rest. |
| `native.rs` `NativeError::Closed` where a turn's channel closes with no frame at all and `spine.rs` after a restart | Left as is: there is no child to ask (the process is gone or the app restarted) and the sentence says only what is known. |
| `quota/probe.rs` `Connection::start`: the usage reader's child runs with stderr discarded, so a failed read is only "Claude Code did not answer just now" | Open. The account card no longer depends on it for the sign-in (it asks `claude auth status`, see [unknown-state-shown-as-pending.md](unknown-state-shown-as-pending.md)); the reason for a failed reading is still not kept. |
| `provider_auth.rs` `status_in`: stderr discarded | Deliberate: that file's rule is never to retain provider output or account details; it reads one boolean. |

## What would settle the original Mac's cause

Nothing on a 1.2.1 Mac recorded it, which is this class. From that Mac, these files would:
`~/Library/Application Support/com.richos.app/conversation-ledger.jsonl` (each turn's recorded
reason) and Claude Code's own session transcripts for the leases, under `~/.claude/projects/`
in the subfolder named after the company's coordination folder (the lease's working directory,
`native.rs` `spawn_with_tools(bin, &profile.coordination, ...)`), the `.jsonl` files of that day,
which record an API error the child met before it ended. From the build with this fix on, the turn's reason in
the ledger carries the child's own last lines.
