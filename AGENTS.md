# Verification instructions for every agent

Before launching, retrying or resuming verification, read
[the verification retry procedure](docs/development/verification-retries.md).
It applies to this entire repository, including delegated work and reviews.

- Inspect existing results and source identity before starting tests.
- Run the failed, timed-out, refused or unrun unit first. Do not put already
  passing checks ahead of the unresolved blocker in a retry.
- Preserve the runner's default parallelism and resource guards. Do not quietly
  override worker counts, shard counts, CPU limits or timeout settings.
- Reuse applicable passing evidence and verify complete coverage. Do not restart
  the full selection merely because one unit failed or a run was interrupted.
- Before any broad rerun, record what invalidated the old results and why the
  smallest retry cannot resolve the gap. A full-proof request is not an order
  to repeatedly rerun unchanged passing tests.
- Never relabel a timeout, cancellation, refusal or skipped suite as a pass.
- Stop verification when the required evidence is complete. Documentation-only
  follow-ups do not justify repeating unrelated runtime suites.

These rules do not authorize bypassing required release gates, changing test
assertions, weakening safety controls or claiming a single clean invocation
when the result was reconciled across multiple runs.

# Spoken reply changes

Before changing the voice path, decoder packaging or conversation startup, read
[the spoken-reply latency record](docs/development/spoken-reply-latency.md).
A pinned decoder and cached calibration do not prove first-use initialization.
Keep real decoder startup off the first user utterance and verify the application
with `voice-walk.py --expect-warmed-decoder`. Preserve complete last-word-to-playback
evidence and do not substitute a warm-cache benchmark for first-use coverage.
