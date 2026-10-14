# Spoken replies: remove confirmed waits and keep thinking

The decoder now initializes before capture opens. Conversation replies use Opus at
low effort under the user's October 14 instruction; background work keeps high
effort. Thinking remains enabled. These changes repair confirmed delays, but the
final VM run's 10.573-second full spoken wait does **not** prove instant replies
are restored. Physical app end-to-end latency remains unmeasured.

## Final implementation

- Keep the already landed cached continuity brief and combined engine preparation.
  Ordinary and spare desks avoid a hidden continuity model turn on their first send.
- Conversation leases use low effort; Work leases retain high effort. Preserve the
  user's model selection. The UI no longer describes every Opus conversation as
  high effort. The actual-spawn regression checks both lease roles.
- Readiness silently initializes the verified speech decoder and selected model.
  Capture joins the same initialization before opening. Cache success only, keyed
  by verified decoder and model identities. A failed decode retries, a changed
  identity initializes again and temporary audio/output is removed in either case.
- Pocket TTS generates and plays PCM blocks while it produces the rest of a sentence.
  Stateful sample-rate conversion preserves the seeded waveform. Cancellation stops
  generation and a failure after partial playback cannot repeat the sentence through
  a fallback. Keep the existing turn-taking, echo and barge-in controls.
- Keep Opus thinking enabled. The transient thinking-disable change was removed after
  the user rejected it. No app-level timing benefit from disabling thinking was
  measured. The earlier startup placement request was withdrawn after it was
  committed; readiness initialization and its capture join are the final behavior.

Read [the durable latency guide](../development/spoken-reply-latency.md) before
changing this path. AGENTS.md, the architecture guide, STREAMING.md and the voice
verification guide all point to it. A version check, cached calibration or verified
weight hashes do not prove the decoder has initialized. Run it before user audio.
The user's acceptance of a normal short synthesis wait is not an instruction to
optimize that interval while ignoring the full reply wait.

## Measurements with thinking enabled

The previous complete new-code walk used app `2815d79d2` without the new decoder
initialization, a 5 GiB guest and the recording identified below. The final proof
used app `edec90ecc`, a 4 GiB guest under the now-landed official runner default,
the same recording and no provider launcher override. These are separate
observations, not a controlled estimate of the warm-up's performance effect.

| Phase | Previous new-code VM | Final warmed-decoder VM |
| --- | ---: | ---: |
| Recorded fragment end to handover | 3.824 s | 5.925 s |
| Handover to model send | 0.282 s | 0.446 s |
| Model send to first text | 1.962 s | 3.331 s |
| First text to first complete sentence | 0.040 s | 0.405 s |
| First sentence to first played sample | 0.209 s | 0.466 s |
| Handover to first played sample | 2.493 s | 4.648 s |
| Recorded fragment end to first played sample | 6.317 s | 10.573 s |
| Hidden continuity records during the spoken turn | 0 | 0 |
| Visible model thinking frames | 3 | 3 |

The difference between the earlier 1.962-second model wait and 6.317-second full
wait was mainly recognition: decoding the 7.536-second admitted fragment took
3.797 seconds. The two-second end-of-turn silence overlaps recognition. It must
not be added to that whole recognition interval a second time. The final VM
initialized its decoder in 10.227 seconds **before** capture opened, then decoded
its real speech fragment in 5.891 seconds. Its warm-order assertion passed; its
remaining recognition wait cannot be relabeled cold initialization.

The physical Mac's silent, file-only decoder experiment used the exact same pinned
binary, model and production flags. Its first invocation on the 40-second padded
recording took 9.764 seconds, then 0.932 and 0.879 seconds. The silence produced an
extra word at the end, so this is startup timing evidence, not perfect recognition
of that padded fixture. A later initialized-host check took the first 7.536 seconds
of the same WAV, matching the admitted fragment duration, and decoded the correct
text in 1.170 and 1.062 seconds. This slice is not asserted bit-identical to the
app's processed VAD fragment. It establishes that VM recognition speed does not
predict the physical Mac's speed; it does not establish physical app reply latency.

The prior matched effort experiment remains in
[the earlier investigation](2026-10-14-spoken-reply-delay.md): high effort's model
wait was 5.974 seconds versus 3.364 with the effort flag omitted. One sample per
condition is an observation, not an every-request guarantee. Low effort with
thinking enabled produced typed first text in 0.978 seconds in the earlier new-code
walk and 1.952 seconds in the final walk. First played speech may begin with a
model-generated acknowledgment; it is not a measurement of a completed useful plan.

## Verification and exact inputs

The tested app is `edec90eccfb3d7934597dad836ed4544cfcd8768`. The later branch tip
`b793e90ab` restored exactly that complete source tree after removing the transient
thinking and startup-placement changes. The following changes only document the
result and incorporate local main's already-landed VM resource guards.

- Engine archive SHA-256:
  `9042a4d394f7ffd1e05a8c20180147e762853f75f6bc4edfa07ac5b399d9549d`.
- Packaged whisper.cpp 1.9.1 decoder SHA-256:
  `fe7b744a4b31f587a52b51a426a74b2727f4348e2260360cee4151e76a715440`.
- Pinned small.en model SHA-256:
  `c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d`.
- App zip SHA-256:
  `9dc16dbd5a360e41edf0bf3ee5ed70b16f675e83f9585ebc3089ebc105145f52`.
- Injected recording SHA-256:
  `8c523f97b0b6b191ed1c8a3642af1680f243d5e6d42271ccde5b243a6425f369`.
  It retains the earlier business statement but uses a new file-only synthesized
  recording because the preceding task's original fixture was removed. It is not
  a recording-matched comparison with the older investigation.
- Provider 2.1.299, actual audited Opus/low leases and three visible thinking frames.
  Default CPU 4, RAM 4096 MiB and display 1680 by 1050. No resource guard or deadline
  override. Guest output muted; no host microphone, speakers or app UI used.

Reconciled passing coverage:

- 321 unchanged voice library units passed with five ignored checks preserved as
  ignored. The new initialization regression separately passed clone reuse,
  changed identities, failed initialization recovery and cleanup.
- The explicitly pinned real Pocket test passed early PCM, cancellation, later
  reuse and equality of the seeded streamed waveform prefix with the buffered
  waveform. Its four-sample lookahead remains a conversion detail, not a latency
  justification. Conditional live checks are not counted as passes.
- The role-effort actual-spawn regression, product model-profile regression and
  five model-settings UI cases passed. Prior ECS, spare and combined preparation
  evidence is reused only for unchanged implementation inputs.
- All 28 voice-walk parser cases passed, including initialization before capture,
  rejection of initialization after capture and complete latency accounting.
- Normal code commit checks passed, including strict changed-source lint and the
  selected quick checks. The 78 heavier landing checks remain Rich's release gate.
- One clean final app walk, `walk-6c549f89347d`, passed all 12 steps and exited 0.
  Plan: first-run, identity, no-decoder, runtime, stage, relaunch, ready, rich-voice,
  new-conversation, heard, spoken and split. It required
  `--expect-warmed-decoder`, one intact spoken turn and the fixture's key words.
  No hidden turn, no early speech and actual Peter Yearsley Pocket speech were
  verified. Boot took 52.3 seconds, scenario 231.8 and cleanup 3.8. The app quit,
  VM stopped and owned clone was deleted.

The broader verification is reconciled across checks, not one clean full-suite
invocation. Retained failed attempts include the initial documentation-count and
American-English guards, the warm-parser fixture lacking its new explicit flag,
a test launcher that refused the existing app settings and two exploratory AX
first-run deadlines. Those exploratory runs produced no thinking-off latency
verdict. Their corrections did not weaken policy or assertions. The final walk
scoped find/type/click to the existing first-window selector, kept the original
deadlines and put the unresolved first-run step first. Complete passing evidence
was retained instead of repeatedly restarting unchanged suites.

Evidence and development artifacts remain outside disposable scratch at
`/Volumes/E1TB/ab/richos-evidence/instant-voice-replies-20261014/`:
`final-result.json`, `final-live/`, `host-stt.json`, `host-stt-segment.json`,
`input/provenance.json`, `final-proof-decision.json`,
`verification-reconciliation-final.json`, test logs and `warm-candidate/`.
The original `live/` result and failed exploratory attempts remain separately.

Landing and installation are distinct from these passing checks. Rich owns the
normal landing gate. Do not claim the user has the fix installed or that physical
end-to-end instant replies are proven by this record.
