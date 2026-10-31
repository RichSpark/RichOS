# RichOS desktop app (`richos/app/`)

The purpose-built, CEO-facing RichOS front-end — a **Tauri** desktop app (Rust core +
web UI), local-first, single-machine, **no relay**. Built to the system architecture:

- The front-end architecture plan, 2026-08-24 (Tauri; RichOS drives the agent
  directly; drop the Nostr relay + its shim; P1 = the runtime spine).
- `wiki/ceo-decisions.md` **§16** (2026-08-31): **the ACP adapter is deleted.** RichOS
  drives the NATIVE `claude` binary over its stream-json stdio. **There is no npm under
  `richos/app/` in anything that ships** — 112 resolved packages across 99 publishers went to 0.
- The session-continuity design, 2026-08-24 (the durable Rich is the APP;
  the Claude session is a swappable compute lease; the ledger is the spine's backbone).
- The RichOS front-end notes (v1 CEO-only / single-machine / no-relay; BYO-Anthropic;
  Rich-organized topic threads over one shared ledger/loro).

The 1.2.0 plumbing candidate connects Loro, ECS, Mega Lander and ASS Kicker from
`richos/engine/` through an app-owned coordination profile. It includes explicit
repository registration, durable work and review receipts, scoped action decisions,
external private knowledge storage and delivered component runtimes. Personal
working context is not bundled. See the [runtime contract](../../docs/architecture/app-engine-runtime.md),
[work contract](../../docs/architecture/desktop-work.md) and
[current verification limits](../../docs/verification/desktop-work-2026-09-16.md).
Installed acceptance and release publication remain separate gates.

## Who reads every string in here

**Non-technical CEOs, based in the US** (CEO, 2026-08-29 — "the main or at least the initial target
audience"). So **every user-facing string in `richos/app/` is American English**: labels, status words,
empty states, error and permission copy. Not code identifiers, not CSS custom properties, not
protocol values — those stay as they are.

Verified 2026-08-29: no British spelling appears in any shipped string in `richos/app/ui` or
`app/crates/*/src`. Every hit for *color*, *behavior*, *cancelled*, *unrecognized* and the rest is
in a comment or is the constant `STOP_REASON_CANCELLED`. The rule exists to keep it that way.

One open question, not a defect: `timeline.js:1140`, `timeline.js:1414` and `main.js:124` format
time with `toLocaleTimeString(undefined, …)`, which follows the **operator's machine locale** — so
the same build shows 12-hour time on a US Mac and 24-hour on a European one. Whether the product
declares `en-US` or keeps respecting the machine has never been decided. See
`richos-hq/wiki/ceo-decisions.md` §13; the full wording rule is §1.1/§17.5 of the Codex-inspired
conversation-UX brief.

## Why `richos/app/` (not `richos/tools/`)

`richos/tools/` holds supporting utilities (the extension, the transcription service, the
dictation HUD). This is the **primary product delivery surface** — the system architecture
calls it "the RichOS DESKTOP APP," a peer of `richos/engine/`. Loro is now an engine component. So it
lives under the product folder at `richos/app/`.

## Layout

```
richos/app/
  Cargo.toml                 workspace: richos-core (fast, native-dep-free) + richos-voice
  crates/richos-core/        the runtime SPINE — UI-agnostic, fully unit-tested
    src/native.rs            the COMPUTE-LEASE client: stream-json stdio to the native
                              `claude` binary. No adapter, no Node, no npm (§16). Carries
                              the scoped permission desk, the loud
                              startup handshake, and the between-turn lane
    src/entity.rs            the ENTITY scope + privacy boundary (ECS §3.2-3.4): validated
                              entity ids, the PER-USER registry (read from `entities.json` in
                              this install's own config dir — see docs/entity-registry.md;
                              EMPTY until its owner registers something, and it was a `const`
                              table of one man's six companies until 2026-09-04), fail-closed
                              repository-root resolution, and the IMMUTABLE ThreadBinding
                              (private fields, crate-private constructor — obtainable only
                              from the ledger)
    src/ledger.rs            append-only conversation + action ledger (crash-safe), now
                              entity-scoped: every thread has one immutable home entity,
                              every turn carries it, and no event from one entity can render
                              in another entity's thread; each assistant delta now persists
                              its shared-sequence position, so a turn's text RUNS (and the
                              tool calls between them) survive a restart
    src/thread.rs            topic threads as VIEWS over the one shared ledger, within an entity
    src/timeline.rs          the TYPED TIMELINE (UX §12): the TimelineItem union projected from
                              the ledger + the machinery journal, entity scope on every record,
                              and visibility as a GATE (Timeline is not Serialize; the only
                              path to a webview is view(mode), which drops what the mode may
                              not see and removes the technical detail it may not read)
    src/reprime.rs           re-prime payload + the LoroContextCompiler Tier-C seam contract,
                              and the EvidenceLookup Tier-E seam beside it
    src/loro.rs              the Tier-C seam IMPLEMENTED: compile a slice, re-assert its lane —
                              and RETAIN its provenance (SliceProvenance: which record each
                              line of the injected memory came from), which is what lets a
                              later correction name a record instead of guessing one. Nothing
                              is retained for a slice that was refused
    src/evidence.rs          TIER E — the EVIDENCE LOOKUP: the CEO's own files, labeled as
                              files, consulted ONLY when the compiled slice's own coverage says
                              memory did not answer (none | adjacent). It is not a fifth loro
                              source and it is never company memory; the block arrives verbatim
                              from tools/richos-service and takes no characters from the memory
                              lanes' budget. Writes nothing, promotes nothing, retains nothing
    src/correction.rs        the loro WRITE loop: propose, ASK the CEO, then write
    src/cognition.rs         the swappable compute-lease trait (+ MockCognition), LeaseFactory
    src/stream.rs            live UI-facing turn events (streaming deltas + turn/proactive state)
    src/live.rs              the THIRD event family (UX §13): seven typed live-work events,
                              four deliberately NOT emitted, and the fence + visibility gate
                              that make a payload impossible to build from a loose thread id
    src/worker_events.rs     the engine's worker-lifecycle stream, read and joined BY
                              IDENTITY to the Task call that spawned the run — session-scoped,
                              because agent_id is not unique across sessions
    src/machinery.rs         the SECOND event family: every non-text agent frame, routed not
                              dropped, normalized + merged by toolCallId (rich://machinery)
    src/journal.rs           the machinery JOURNAL — separate store, per-thread, day-sharded,
                              Tier A never evicted / Tier B a rolling raw window
    src/steering.rs          the CEO's two MID-TURN controls (UX §9.2/§9.3): an append-only
                              intake log the stop/steer commands can write to WITHOUT the
                              spine lock (which a running turn holds for its whole length),
                              plus the lease cancel seam. Not a second source of truth —
                              every record either becomes a ledger event or is refused
    src/spoken.rs            THE FLYWHEEL'S AUTOMATIC TRIGGER: what makes an utterance a
                              correction — a `not`-pivot repair frame, the shipped
                              ceo-decisions.md §7 term gate (ported), and a record anchor
                              carried as EVIDENCE not as a gate. Detects; never writes.
                              Measured: precision 1.000, recall 0.941 over 149 invented
                              utterances (docs/measurements/spoken-correction-trigger-2026-08-30/)
    src/belief.rs            THE LORO DESK'S PROPOSER: what makes an utterance a correction of
                              a recorded BELIEF, and — the load-bearing half — WHICH record it
                              corrects. Same `not`-pivot frame as spoken.rs (one extractor,
                              two gates) with a near-opposite judgement: a different VALUE
                              rather than a mishearing, and a reference resolved to exactly
                              one record or nothing filed. Detects; never writes.
                              Measured: precision 1.000, recall 0.971 over 147 invented
                              utterances (docs/measurements/loro-correction-trigger-2026-08-30/)
    src/staging.rs           where a detected correction LANDS: durable candidates
                              (append-only JSONL, fsync per record) and §7's three outcomes.
                              `confirm` is the only path to a vocabulary write, and it goes
                              through `richos-service learn-term` rather than a second
                              implementation. Held as an Arc BESIDE the spine lock, so an
                              ask raised during a turn can be answered during that turn
    src/spine.rs             queue-not-interrupt + turn-boundary rotation + crash recovery +
                              the proactive-attention seam + stop settlement at the boundary
                              + the correction trigger at submit_prompt step 1b (every CEO
                              utterance, voice or typed, with no command typed)
    src/setup.rs             FIRST-RUN SETUP — OPTION D. The two executables a customer's Mac
                              does not have, detected, fetched and verified: Claude Code by
                              driving ANTHROPIC'S OWN installer (never redistributing their
                              binary, never re-signing it), and the engine directory as a
                              pinned release asset. §19's launch blocker in its own words —
                              "today RichOS runs on his Mac and would not run on anyone
                              else's" — and this is the half that closes it. Digest before
                              extraction; the RESOLVER'S OWN shape predicate after it; an
                              atomic swap that keeps the old engine until the new one lands;
                              and an unset pin that REFUSES rather than fetching whatever a
                              URL returns. Proved end to end on a clean HOME in
                              docs/verification/first-run-setup-2026-09-02/
    src/config.rs            durable CEO preferences: company_name, the assertiveness dial
    src/worker_status.rs     the optional AI-worker drill-down (reads the engine's event logs)
    src/upstream.rs          UPSTREAM MODEL-API FAILURE, first-class (open-items row 3.30).
                              `529` overload and `429` quota classified apart, from the
                              vendor's own structural tokens rather than its English; the
                              BOUNDED, VISIBLE retry budget; and the what-survived statement
    src/work_gate.rs         IS RICHOS DOING ANYTHING RIGHT NOW — the decision the updater
                              never asked (RICH-TODOs row u1). Three readings in, one verdict
                              plus the sentence that explains it out. Holds no handles and
                              makes no syscalls, so the shell takes the readings and this
                              decides; unknown blocks exactly as busy does, and the one
                              exception (no lease, so no workers to see) is reported in words
                              rather than assumed away
    src/feedback.rs          the in-app feedback channel, LOCAL HALF ONLY: the 1/2/3/0 rating
                              prompt, its one-file store, and the VERSIONED CLOSED VOCABULARY a
                              report is assembled from — FeedbackPayload has no String field at
                              any depth, so a user's specifics are unrepresentable rather than
                              filtered. Nothing in it sends anything and there is no queue
    src/util.rs              the shared id and clock helpers, and nothing else
    examples/native_roundtrip.rs   headless proof of the real round-trip
    examples/native_failure_modes.rs THE LOUD-FAILURE PROOF (§16): five ways the compute
                              lease can fail, each shown to be named, bounded and never a
                              degraded success — including the REAL binary rejecting a flag
                              it does not know. Costs no API turns
    examples/rotation_roundtrip.rs headless proof of rotation against the real binary
    examples/live_events_roundtrip.rs both families side by side on one real turn
    examples/worker_status_demo.rs   what the drill-down reads, against real event logs
    examples/loro_reprime_demo.rs / loro_correction_demo.rs  the Tier-C read and write loops
    examples/watermark_roundtrip.rs  the rotation trigger, reading the agent's own usage
    examples/ledger_projection_digest.rs a content-free digest of the projection a ledger
                              file replays into. Run it with two builds against the same
                              file and diff: identical output is what a claim that a reader
                              change left every existing history alone is made of
    tests/ledger_forward_compat_tests.rs holding one line — NOTHING ALREADY
                              READABLE MAY BECOME LESS READABLE. Two committed fixtures
                              carrying every record shape the shipped builds wrote, pinned
                              against goldens captured BEFORE the tolerant reader existed;
                              then the destructive half — a record from a newer RichOS, a
                              torn write, a bad byte, a tag no build could have written —
                              and the anti-vacuous test that an unmodified ledger skips
                              NOTHING
    tests/intake_forward_compat_tests.rs holding the SAME line over the intake
                              log — the file that holds what the CEO TYPED before it becomes
                              a turn. A record from a newer build, a torn write, a bad byte
                              placed FIRST (the reader used to stop there and lose every
                              request below it), a tag no build could have written; the id
                              can never be re-issued; compaction can never delete a record
                              nothing could read; a planted secret reaches no report; and
                              the anti-vacuous test that an untouched log skips NOTHING and
                              says NOTHING
    tests/correction_forward_compat_tests.rs holding the SAME line over the
                              CORRECTION DESK — the file that holds what he was ASKED and
                              what he ANSWERED. The desk is an event log, so losing the
                              LATER record loses the ANSWER: a correction he confirmed
                              reverts to `AwaitingCeo` and is put in front of him a second
                              time. A record from a newer build, a torn write, a bad byte
                              (the reader used to stop there and lose every answer below
                              it), a tag no build could have written; a proposal whose
                              answer might be missing is held back rather than asked again,
                              and one whose answer DID load never is; a permanent decline
                              survives either of its two records being unreadable; the id
                              can never be re-issued; a planted secret reaches no report;
                              and the anti-vacuous test that an untouched desk holds NOTHING
                              back and a real proposal still reaches the writer
    tests/entity_binding_tests.rs entity-scope tests: the cross-entity leak NEGATIVE
                              CONTROL (proven failing with the guard removed), immutability,
                              the fail-closed unbound legacy thread + its one-way explicit
                              adoption, the activation fence, and restart
    tests/entity_registry_tests.rs that the registry is the PERSON'S OWN, read from
                              their own entities.json — empty by default, fail-closed on every
                              unknown root, and no existing thread orphaned by the migration
                              rather than four directory names, including the multi-root
                              proof (`richos` + `richos-hq` -> one entity id) and the
                              string-prefix trap beside it
    tests/spine_tests.rs     spine invariant tests (no live Claude needed). One is the
                              nightly D4 pair: a failed TEXT turn and a failed VOICE turn are
                              classified identically, which is what makes the operator line
                              impossible to write for only one surface
    tests/native_input_provenance_tests.rs native input provenance witness.
    tests/terminal_session_tests.rs native session continuity and overlapping-input regressions.
    tests/rotation_tests.rs account-resume, native model-control and quota-notice tests.
    tests/questions.rs       durable question sets, revisions, delivery, engine witnesses and the app-run ask gate
    tests/instructions_update_tests.rs a new version's improved rich.md/stu.md reach the user:
                              an unchanged copy updated silently, a changed one asked about once
                              (OK writes the update, No is not asked again for that version), a
                              copy from before the record recognized by the earlier versions
    tests/action_ledger_tests.rs action-ledger WRITER tests (the ledger is non-empty
                              at runtime; CEO-facing actions cross a rotation; machinery
                              stays out of every priming prompt)
    tests/machinery_tests.rs machinery routing/retention tests, driven by native wire
                              frames actually measured against the binary
    tests/steering_tests.rs  stop/steer tests (UX §9.2/§9.3). Includes the CONCURRENCY
                              proof: the spine goes behind an Arc<Mutex<..>> exactly as the
                              Tauri shell holds it, a real turn runs on one thread, and the
                              test asserts try_lock FAILS at the instant the stop is pressed
                              — so it cannot silently start passing for the wrong reason.
                              Also: a turn the CEO stopped is never crash-replayed, and a
                              stop request that outlived the process is applied at startup
    tests/native_onboarding_grant_tests.rs real-child checks for scoped visible-turn grants and safe revocation.
    tests/native_cancel_tests.rs interrupt tests against a REAL CHILD PROCESS over real
                              stdio (a POSIX-sh fake `claude` the test writes itself), in two
                              variants: compliant, and deliberately deaf to the interrupt
    tests/replay_work_host_tests.rs recorded-provider checks: native turn endings and
                              late command notifications never manufacture job completion,
                              a user reply or another model prompt. Separate handoffs
                              use the same independent Stu session.
    tests/job_execution_owner_tests.rs preserves Stu's reports and inbox receipts
                              across ordinary Quit and restart.
    src/skills.rs             THE SKILLS RichOS gives its inner Rich — the on-demand half of
                              src/doctrine.rs. Rendered as a PLUGIN into the app's own
                              directory and passed with `--plugin-dir`, because a project
                              `.claude/skills/` does not reach a child started with
                              `--setting-sources ''` and a plugin path does (measured; the
                              engine's own 28 skills never reached it for exactly that
                              reason). A missing plugin dir is accepted by the binary in
                              SILENCE — exit 0, clean handshake, `plugins: []` — so the
                              loudness is ours: `preflight` refuses before the spawn, and the
                              reader thread reads `system/init.plugins` for the case only the
                              wire can see. Ships ONE skill, American English
                              (docs/verification/inner-doctrine-skills-2026-09-06/). Now
                              ships TWO: American English, and the bootstrap interview —
                              which is NOT the engine's copy of that skill, because that one
                              fills a CLAUDE.md the child never reads, spawns a Dean this
                              session cannot resolve, and runs scripts a customer has no
                              repository for
    src/company.rs            THE COMPANY LAYER — what the CEO said about the company a
                              thread belongs to, read from
                              `~/myrichos/companies/<id>/company.md` at prime time and
                              appended to the scoped assertion. The priming turn re-fires on
                              every thread change and thread -> entity is one-to-one, so it
                              is ALREADY scoped to the company boundary; what was missing was
                              content. It creates nothing under that folder — a reader that
                              made its own source could never report the source missing.
                              Over budget (8192 bytes) is a REFUSAL, never a truncation:
                              half a house rule is a wrong house rule and it would be wrong
                              silently. Six states, because "the folder is not there", "the
                              file is not there", "it is empty" and "it could not be read"
                              call for four different fixes
    src/onboarding.rs         WHETHER THE CEO GETS ASKED ABOUT HIS BUSINESS, and the refusal
                              that keeps the answer honest. The interview's trigger used to
                              live in `richos/engine/CLAUDE.md.template:25` — a correct instruction
                              in a file nothing renders — so this ISSUES the offer from the
                              priming turn rather than making a skill available and hoping.
                              It is derived from a file on disk, never from a "seen it" flag,
                              and it RETIRES ITSELF the moment that file has substance. The
                              record it keeps holds ONE field, a declination, and a test
                              exists whose only job is to refuse a second one. The offer and
                              the interview both say plainly that nobody has been hired:
                              measured, an attempt fails loudly rather than silently
                              (`Agent type 'dean' not found`), but two cells are a behavior
                              and the structure is that neither text reaches for staffing at
                              all (docs/verification/onboarding-honesty-2026-09-06/)
    examples/onboarding_probe.rs what THIS install will do about onboarding, one line per
                              company, plus `--block <entity>` for the exact bytes the
                              priming turn would carry
    tests/doctrine_sentinel.rs LIVE test, #[ignore] by default, and a RELEASE GATE rather
                              than a one-off: does the standing instruction RichOS renders
                              (src/doctrine.rs) actually reach the model? Two cells against
                              the real binary through the PRODUCT'S own arg vector and
                              locator, differing in exactly one thing — the contents of the
                              file --append-system-prompt-file names. The sentinel carries a
                              BEHAVIOR as well as a token, so a null result distinguishes
                              "never read it" from "read it and did not echo". It costs a
                              real API turn, so `ignored` is what libtest prints — a not-run,
                              never a pass; run it with --ignored against the binary a
                              release ships against
                              (docs/verification/inner-doctrine-live-2026-09-06/)
    tests/between_turn_tests.rs for techy-mode §1.5 gap #1, also against a REAL
                              CHILD PROCESS: the frame the agent emits at session start and
                              after a turn's result — which used to hit no sink at all — is
                              parked, deduplicated by the last_session_meta slot (on
                              meta_identity, because the frames differ by uuid), and drained
                              with turn_id: None. The fourth pins spike caveat C3: the
                              derived context measurement cannot exist before turn two
    tests/between_turn_thread_tests.rs for the same gap end to end: a real
                              NativeCognition's session-start frame reaching the journal and
                              the technical view, re-prime machinery that is recorded and
                              structurally cannot render, no session id on the lane's wire,
                              the honest empty lane, and the retention window covering it
    tests/bug_report_tests.rs for Bust a bug (CEO §115, round 21): names (in
                              any capitals, in every alphabet, with an accent
                              encoded either Unicode way), file
                              paths (any word with a slash or a backslash, relative
                              ones and two plain words too, but not a web address
                              without a backslash, "and/or", "w/", "w/o", "n/a", "24/7"
                              or an all-digit fraction or date; the whole line it is
                              on is left out), email addresses and every name Rich finds private
                              (in any capitals, one word or several, whatever rule applies
                              to the app's own names; kept for every later change; overlapping ones left out
                              together) are replaced by stand-ins that
                              keep what they replaced for the user's eyes only; the card's
                              heads-up on the user's own changes is the same scrubber's
                              answer (`private_in_edit`); Rich is
                              given the screen's words and a picture of the window to
                              check against (one stream-json message), and his write-up
                              is read out of his answer; a report he could not check
                              (an error, a timeout, no report in his answer, a section
                              of his last pass that is not all there) is never
                              offered for sending: it is kept on this Mac and he is
                              asked again until he answers; the order is the CEO's (§115):
                              the scanner first, Rich last (every prompt asks him for the
                              report with this user's private details already left out,
                              and his last pass, `finished`, given the report before and
                              after the scanner, puts back what it hid that is not private
                              and cleans up what it left), the card is his words and the issue is the card word
                              for word, nothing after him changing a word; what is sent is
                              decided in one place (`at_send`): the words Rich gave last,
                              and words changed by hand or by telling Rich go through the
                              scanner and then Rich first, his version shown before
                              anything goes when it differs, or wait on this Mac when
                              Claude cannot answer; a line break typed in a paragraph
                              joins no two words; a send
                              files one issue with the token read at send time; a failed
                              send is kept on this Mac, survives a relaunch and goes out
                              when it is due; canceling one is confirmed (off this Mac, or
                              the issue it had already become, or an error and still
                              waiting). Fake transport and credentials throughout
    tests/feature_request_tests.rs for Request a feature (CEO 2026-10-11, round 24):
                              an idea goes through Bust a bug's own machinery and is posted
                              as a discussion under Ideas (GraphQL lookup of the repository
                              and the `ideas` category, then `createDiscussion`), never to
                              the issues endpoint; the user's own sign-in when there is one,
                              the reporting account otherwise; an account that may not post
                              there (FORBIDDEN, INSUFFICIENT_SCOPES, 401, no Ideas category)
                              waits, refused; offline it waits and posts by itself when back
                              online; private names are left out by the same scanner and
                              Rich last, and nothing in the post calls it an issue or a bug.
                              A stand-in GitHub throughout
    tests/feedback_no_outbound_tests.rs asserting an ABSENCE: no transport in the
                              module's shipping code, no network-capable dependency in the
                              crate, no other module consuming the feature, and an approval
                              that lands in one file with no sibling left for anything to
                              pick up — plus three added with the SURFACE, because the first
                              four were written when the module had no caller: no transport
                              in the six Tauri commands OR anything they call (the call graph
                              is walked, seeded from `generate_handler!`), no network
                              primitive anywhere in the shipped web layer, and the positive
                              half — `feedback_record` still compares the rendered report
                              against what the webview says it showed him. Each proven to
                              FAIL when broken
    tests/launch_no_outbound_tests.rs holding the LAUNCH RECORD to the identical
                              standard, at the CEO's direction — it is a record of his own
                              working life, so "local only, never outbound" is asserted the
                              same four ways (no transport in `launch.rs`, no network-capable
                              dependency, no other module consuming it, one file and no
                              sibling after a whole run) plus the surface: no transport in
                              the two Tauri commands or anything they call, no network
                              primitive in `splash.js`/`splash-library.js`/`main.js` (which
                              the feedback suite's web check does not name), the window
                              injection pinned to a frozen one-field verdict rather than
                              merely scanned, the local-bucketing offset proven to come from
                              the CALLER with no timezone read in Rust, and the shape handed
                              to the webview pinned field by field. Two positive controls,
                              because a scanner over an empty corpus reports clean
    tests/feedback_surface_tests.rs that WRITE the three fixtures the browser suite
                              checks `richos/app/ui/mock.js`'s copy of this feature against — the
                              wording and the whole vocabulary, six selections with the exact
                              block each renders, and three stored entries taken through a
                              real store round trip. Regenerate with RICHOS_WRITE_FIXTURES=1;
                              without it they compare and fail on any drift
    tests/spoken_precision.rs THE MEASUREMENT, pinned as a test rather than quoted in a
                              brief: TP 32 / FP 0 / FN 2 / TN 115 over the invented corpus,
                              plus the anchor-as-a-gate counterfactual that demoted it
    tests/belief_precision.rs THE OTHER MEASUREMENT, pinned the same way: TP 34 / FP 0 /
                              FN 1 / TN 112 over 147 invented utterances, plus the
                              topic-condition counterfactual that earns it (precision 1.000
                              -> 0.739 with the condition off, recall unmoved)
    tests/belief_trigger_tests.rs for the loro completion criterion, driven through
                              the REAL read seam: a slice -> CliContextCompiler::interpret ->
                              SliceProvenance -> submit_prompt -> a proposal with the RIGHT
                              ref on a real desk. Plus the fixture richos/app/ui/ renders, checked
                              against the live detector so a screenshot cannot go stale
    tests/heard_precision.rs THE THIRD MEASUREMENT, and the first that is not 1.000:
                              TP 35 / FP 1 / FN 3 / TN 117 over 156 invented heard/sent
                              pairs, precision 0.972 / recall 0.921. Carries the
                              grammar-word counterfactual that earns it (FP 1 -> 18 with the
                              condition off, recall unmoved), the `emitted`-vs-`text`
                              counterfactual, and a cross-match probe over 24,058 offers of
                              a send against a dictation it did not come from
    tests/heard_trigger_tests.rs for the third completion criterion — a dictation
                              silently edited before sending puts a candidate on the desk
                              with nothing said, over a REAL journal on disk in the format
                              richos/tools/richos-hud/dictation-flywheel.patch writes. Plus the
                              fixture richos/app/ui/ renders, checked against the live detector
    tests/spoken_gate_agreement.rs the ANTI-DRIFT pair: §7's gate has two implementations
                              (this crate and richos/tools/richos-service/lib) writing into ONE
                              vocabulary, so both assert against one generated fixture. It
                              covers the HUNK REDUCTION too since 2026-08-30 — heard.rs
                              ports capture.js's `tokenReplaceHunks`, and a divergence there
                              changes WHICH PAIR is learned rather than whether to ask
    tests/spoken_trigger_tests.rs for the completion criterion — speaking a
                              correction records it with no command typed, ordinary
                              conversation stages nothing, internal traffic is never mined
    tests/timeline_tests.rs  typed-timeline tests: the cross-entity machinery NEGATIVE
                              CONTROL (both clauses proven failing when removed — one leaks
                              a row, one leaks THROUGH the toolCallId merge), the one shared
                              per-turn sequence live and after a restart, the visibility
                              gate, the items that are never invented, and the worker join
                              — and, since 2026-09-19, that a turn which answers the same thing
                              TWICE says it once (Ray's candidate-.12 defect C: a lease replaced
                              mid-turn under the same turn id answered the prompt again, so the
                              Mac drew the greeting twice where the phone drew it once). Both
                              runs are retained; the repeat renders in no mode.
                              with its session clause
    tests/live_event_tests.rs additive-family tests (§13): the four original events
                              asserted BYTE-IDENTICAL with and without the new family, the
                              wire and the reload agreeing field by field, phase honestly
                              `unknown`, the states that are never emitted, the
                              cross-entity fence on every payload, and a pressed button's
                              words never drawn as his
    tests/company_layer_tests.rs driving a REAL spine and asserting on what the
                              lease was actually primed with, never on the renderer: what the
                              CEO said about a company reaches the priming turn, another
                              company's file never does, an over-budget file arrives as
                              NOTHING rather than as half of itself (a HEAD-MARKER and a
                              TAIL-MARKER make a truncation show up as truncation), and
                              learning where the central folder is re-primes a lease that was
                              already running. The two negative controls come first because
                              they are what make the positives mean anything
    tests/resident_front_desk_tests.rs for the CEO's "any number of conversation
                              threads, each holding one front desk": a thread's lease is
                              PARKED alive rather than killed when another thread speaks, so
                              coming back is the same provider session with no re-prime; a
                              message to a conversation that is not the one on screen is
                              answered rather than refused, mid-turn included; a returning
                              desk brings its own context measurement; past the cap the least
                              recently spoken desk is retired and only that one; and moving
                              the central folder un-primes the PARKED desks too, which is the
                              defect residency would otherwise have introduced. Its double
                              reports `requires_thread_isolation()` true — `MockCognition`
                              reports false and never comes down this path — and its `Drop`
                              is the only honest witness that a parked desk was not quietly
                              killed. Past the cap a desk with a running turn is passed over
                              for the next oldest idle one
    tests/idle_front_desk_retirement_tests.rs for idle retirement (follow-ups,
                              decision 3): a parked desk idle past `idle_session_minutes` has
                              its child ended and the desk on screen never does; zero retires
                              nothing; a running turn, input awaiting admission, a background
                              command, an unanswered question or a queued message each keeps
                              its desk, and each released hold lets it go; a retired
                              conversation's next message spawns a desk on the thread's own
                              session and is answered with what was said before
    tests/two_conversations_at_once_tests.rs against the REAL delivered engine for
                              his per-thread ECS cursor: two conversations hold turns open at
                              once and BOTH check point, with the identical sequence on the
                              legacy single cursor REQUIRED TO FAIL in the same test — two
                              threads succeeding proves nothing on an engine that had stopped
                              fencing. Also: a CEO-shaped seat naming another thread is
                              refused, the delivered engine answers the capability AND its
                              prefix, and an unreachable engine answers no rather than
                              assuming support
    tests/onboarding_persistence_tests.rs regressions for verified tool persistence, per-company answers, resumption and MCP transport.
    tests/your_instructions_tool_tests.rs Rich saving the user's instructions (round 36 screen 1): rules added at the end of instructions.md only in a visible turn, extra arguments refused, and the card left for that conversation's turn.
    tests/request_preparation_tests.rs regressions for cancellable preparation, independent replies and startup, rotation, provider retries and restart outcomes.
    tests/restart_continuation_tests.rs restart tests for atomic continuation, uncertain native attempts, preserved control provenance and durable Stop across replay.
    tests/routing_context_tests.rs verifies that classification receives the existing scoped visible conversation, including formatted replies, without hidden inputs.
    tests/onboarding_declination_tests.rs over the two spine methods the first-run
                              notice is built on. `record_declination` shipped with no caller
                              anywhere in the product, so `OnboardingState::Declined` and
                              `DECLINED_BLOCK` were unreachable outside their own unit tests
                              and "not now" could only be said into a conversation that ends.
                              The one that matters drives a SECOND spine over the same files,
                              so the press has to survive a relaunch; the others hold the
                              screen and the priming turn to one derived fact across all four
                              reachable states, and refuse a write that has nowhere to go
                              rather than reporting success over it; the fifth presses "Not
                              now" during boot priming with a message queued behind it and
                              counts the priming turns: no extra one at the next message;
                              the sixth sends a message to each kind of desk after the answer
                              (in use, parked, spare) and finds the decline in each one's next
                              priming; the seventh presses it while a spare is starting and
                              finds no extra priming turn at that spare's first message
    tests/loro_lane_map_tests.rs lane-map tests: the default map is the CEO's six
                              companies, a lane the corpus does not have is DROPPED rather
                              than sent (loro exits 2 on one, which would make every
                              re-prime Unavailable), the cross-entity guard still refuses
                              after a drop, and a thread reading another company's in-repo
                              record is told whose record it is
    tests/evidence_lookup_tests.rs Tier-E tests: the consult gate is EXACTLY none and
                              adjacent, a compiler that reports no coverage label never opens
                              the owner's files, a covered question spends no process at all,
                              the block is injected verbatim BELOW the company-memory heading
                              with the model told above it that the text is data and never an
                              instruction, the spoken form carries the document's name and date
                              and no URL, and the memory lanes' characters are byte-identical
                              with and without a block
    tests/loro_gui_launch_tests.rs for the launch nobody tests on (including the two
                              subprocess children they drive): a double-click
                              carries HOME, USER and PATH and nothing else, and until
                              2026-09-01 the WRITE half of loro read three environment
                              variables it therefore never had. Two of them re-invoke the
                              test binary with env_clear() and cwd=/ so the GUI condition is
                              a real process rather than a value someone passed, and the one
                              that matters most asserts the reader and the writer resolve
                              the SAME corpus — a build where they disagreed would show him
                              a proposal about one record and write to another
    tests/deferred_send_tests.rs
                             for durable input during preparation, exact ordering,
                              one delivery and independent conversation priming claims.
    tests/spare_front_desk_tests.rs
                             for a spare primed before its thread exists, safe company
                              boundaries and unlocked startup/priming with the same shared factory.
                              A stale completed prime is retained without claiming it is current.
    tests/front_desk_priming_tests.rs
                             for preparing the selected conversation before the first
                              message, readiness after repair and accurate startup reporting.
    tests/setup.rs           for FIRST-RUN SETUP (Option D, `src/setup.rs`): the
                              two executables a customer's Mac does not have, fetched and
                              verified, and since 2026-10-07 the video tools (yt-dlp and a
                              pinned speech model) as the third essential. Every failure path is a VALUE rather than a network
                              condition — no network, a 404, a truncated body, a tampered
                              body, an installer that refuses, HTML where a script should
                              be, an archive with the wrong shape or the wrong version, a
                              panic mid-install, and a failed reinstall that must leave the
                              engine he already had. The digest is checked BEFORE `tar` is
                              ever handed the bytes. The positive half of the signature pin
                              runs against the real `claude` ON A MAC and FAILS if there is
                              none — until 2026-09-05 it returned early instead, which is
                              reported `ok`, so it had asserted nothing on every ubuntu CI
                              run since that job landed. Off macOS it is now
                              `ignored, NOT CHECKABLE ON THIS TARGET: …`, because the pin is
                              read by `/usr/bin/codesign`. The negative half needs nothing
    tests/media_tools.rs     for YT-DLP KEPT AT THE NEWEST NIGHTLY (`src/media_tools.rs`,
                              media-tools plan slice 2): a newer nightly replaces the copy, a
                              download that does not match its checksum is deleted, offline
                              keeps the old copy, the old version file survives the swap and
                              goes one check later (review m1), the tools folder sits on PATH
                              after `/sbin` (m2), one tag per check and a 5-minute retry while
                              no copy exists (m3). Fakes only, no network.
    tests/interruption_tests.rs for WHY A TURN ENDED WITHOUT FINISHING
                              (`src/interruption.rs`, the 2026-09-17 nightly's D2). The
                              published nightly held
                              `cognition protocol: "Not logged in · Please run /login"` in
                              its own ledger and told the CEO it had hit a snag, that his
                              work was saved, and that a button would pick it back up — all
                              three false. Each untruth is made unreachable here: a
                              permanent condition is not called transient, no claim about a
                              saved answer is made when nothing was written, and
                              `offers_retry` is false for every class a retry cannot clear.
                              The reason strings are the REAL ones, five of them read
                              verbatim out of the installed Claude Code bundle, including
                              the vendor's one string that carries both families' wording.
                              The transient arm has its own positive control, without which
                              a classifier answering `Unknown` to everything would pass
    tests/worker_attribution_tests.rs that the workers in the prompt are the
                              SERVING SESSION's, derived from the session identity and
                              never from a directory mtime (a decoy dir is present in
                              every case, so "reads nothing" cannot pass by finding nothing)
    tests/upstream_classification_tests.rs pinned to CAPTURED BYTES — the five
                              `API Error: 529` lines from 2026-09-03, four of them carrying
                              the incident's own request ids, at
                              docs/verification/upstream-failure-2026-09-05/. It proves `429`
                              and `529` never present the same way, that classification reads
                              the vendor's structure and not its English, that an overload
                              retries on the CEO's schedule (1, 2, 5, 10, 20, 40, 80 minutes,
                              2026-10-07) and any other fault at most once, saying what it
                              spent
    tests/upstream_turn_tests.rs driving the REAL spine against an injected
                              upstream: the `529` that arrives as an ASSISTANT MESSAGE (the
                              shape that would otherwise complete a turn whose answer is a
                              vendor diagnostic in Rich's voice), the loss statement built
                              from ledger counts, its survival across a cold reopen, and the
                              two positive controls without which the rest is theater — an
                              ordinary broken pipe is NOT dressed up as an outage, and a
                              healthy turn produces nothing at all
    examples/machinery_roundtrip.rs headless proof that machinery is routed AND retained
                              end to end against the real adapter (the run is kept at
                              docs/verification/machinery-roundtrip-2026-08-28.txt)
    examples/loro_install_probe.rs WHICH CORPUS DOES THIS LAUNCH RESOLVE? — both halves,
                              printed, from one resolution. Read-only. Run it under
                              `cd / && env -i HOME=$HOME USER=$USER PATH=/usr/bin:/bin:/usr/sbin:/sbin`
                              and nothing else tells you the truth
    examples/loro_gui_correction_e2e.rs the whole correction desk under that same condition
                              on a SCRATCH machine — provision, propose, confirm, and the
                              record on disk. It stops one hop short of the button, and its
                              header says exactly which hop
                              (docs/verification/loro-write-path-2026-09-01/)
  crates/richos-voice/       VOICE MODE — mic -> whisper -> the spine -> TTS -> speakers
    src/vad.rs               RMS VAD + THE FRAME MATH (16000 Hz, 256-sample frames = 16.000 ms)
    src/bargein.rs           15-of-25 (0.400 s) window on VAD-speech-AND-near-end whenever a
                              canceller exists; 313-frame (5.008 s) consecutive debounce only
                              when there is none; the EchoGate seam
    src/aec.rs               ACOUSTIC ECHO CANCELLATION — 2048-tap PBFDAF (128.0 ms tail),
                              lock-free reference ring, envelope delay estimator.
                              28.0 dB ERLE on a linear path; ~5.5 dB is all ANY linear
                              canceller can reach on this host's speakers+Elgato (measured)
    src/fft.rs               512-point radix-2 FFT, hand-rolled (license: no vendored crate)
    src/endpoint.rs          utterance start/end, pre-roll ring, cough filter, 30.000 s cut
    src/turn.rs              END OF HIS TURN, not of one stretch of speech: utterances are
                              fragments, joined into one turn after 125 frames (2.000 s) of
                              silence since his last word (nightly 54: a 1.406 s pause had made
                              his first two words a turn of their own)
    src/noaudio.rs           post-open silent input: 188 frames (3.008 s) under -80.00 dBFS
    src/voiced.rs            DID THE MIC CARRY A VOICE? periodicity at a human pitch over
                              512-sample windows, hop 256. A transcript is refused unless the
                              AUDIO holds a 6-window (0.112 s) voiced run whose pitch moved
                              by 2.00 % — the gate that stops a silent room being submitted
                              as the CEO's own words (2026-09-04, published v1.0.1)
    src/chunk.rs             streaming sentence chunker + clean output FOR THE EAR
    src/state.rs             the voice-mode state machine (mic state is never a guess)
    src/wav.rs               hand-rolled PCM16 WAV codec + rate conversion
    src/event.rs             rich://voice-state (incl. noAudio) | voice-transcript | voice-error
    src/provision.rs         GETTING THE SPEECH MODEL — the pin table, body sniffing (a captive
                              portal is named as one, not as a byte count), the part-file state
                              machine, and rich://voice-model. Pure plus local filesystem: no
                              sockets, no async runtime, no dependency. The transport is the
                              shell's (src-tauri/src/voice_provision.rs) and decides nothing
    src/{capture,playout}.rs cpal in/out; playout is one continuous, interruptible stream
    src/{stt,tts}.rs         local whisper.cpp (small.en) / Rich's voice behind a trait:
                              Pocket TTS as Peter Yearsley, macOS `say` until it is installed
    src/pocket.rs            Kyutai Pocket TTS (ONNX Runtime, statically linked) with the
                              Peter Yearsley voice: nine pinned files, downloaded on first need; audio streams
                              at each codec block while the rest of the sentence is generated
    src/controller.rs        four threads, CaptureBrain, the half-duplex taint rule
    tests/barge_in_composition.rs  the WIRING: echo defense + real interruptions
    tests/long_speech_on_the_ceos_rig.rs 23.466 s of speech recorded in his room on his
                              microphone, at his -18.1 dBFS, reaches the recognizer to its last
                              frame, over silence and over his room's noise at -44.0 and -36.0
                              dBFS; the VAD floor that climbed inside speech kept 1.376-5.840 s
                              of it and lost the rest (nightly 65). His turn still ends within
                              5.000 s when the room's own noise sits at -36.0 dBFS, with or
                              without a 16 ms or 48 ms click every 0.496 s, and after a short
                              reply over the recorded room of the 2026-09-17 rig at -49.2 to
                              -37.0 dBFS. And no input holds a turn open past 60.000 s, through
                              the turn assembler
    tests/on_it_spoken_at_once.rs on his spoken go the early "On it!" reaches speech
                              the moment it is shown, through the real spine and the production
                              sentence chunker, not at turn end
    tests/rich_voice.rs      voice mode speaks through RichVoice, and with the pinned
                              bundle installed a sentence comes out of Pocket TTS as Peter
                              Yearsley, and the last words of a reply ending "9:00 AM." are
                              heard as said (ignored, never ok, without the bundle)
    tests/rich_voice_only.rs no other voice speaks for Rich or is named for him; a
                              sentence waits while his voice is installed, and the voice the
                              install brings speaks it
    tests/self_voice_replay.rs replaying the candidate-.5 walk (and the near-end onset control): the shipped rule submitted
                             2.224 s of Rich's own answer as the CEO's message; the audible window
                             (queued OR owed OR within the measured device hold, taint re-evaluated
                             every frame) admits none of it; the CEO after the answer and tap-to-stop
                             mid-answer are still admitted (positive controls).
    tests/turn_on_the_ceos_rig.rs a sentence with his measured 1.406 s pause in it, on
                              his room at his -18.1 dBFS (a synthetic voice, and his own private
                              near-end recording), is ONE turn handed over 2.010 s after his
                              last speech; the old rule made two, the first inside the pause.
                              And the floor: an answer arriving while he has gone on after a
                              longer pause is held, superseded, and never said; and a stretch
                              cut at the 30.000 s window is not him stopping; and speech he
                              resumes while Rich's answer is in synthesis is his, and Rich waits.
                              every_way_his_turn_ends is the whole contract in one table: 8 ways
                              a turn ends, each held to one turn per thought, nothing he said
                              lost, Rich silent until he is done. And the onset-to-floor
                              handoff on the app's own clocks (16 ms capture, 25 ms supervisor,
                              10 ms speaker): a sentence ready at any moment of his first word
                              never sounds over him
    tests/echo_path_replay.rs  the CEO's OWN echo path, recorded once and replayed with no
                              device: Mac mini speakers out, Elgato Wave:3 in, the path every
                              synthetic rig here fails to model. Pins that the canceller does
                              not claim confidence on it, and — the positive control — that the
                              recording still reproduces the rule that did. Fixture made by
                              `examples/aec_capture --save`; `--replay` re-reads it silently
    tests/voiced_acceptance.rs REAL `say` speech, every installed voice, through the voiced
                              gate — the acceptance half, so the refusal half cannot pass by
                              refusing everything. Prints the margin and the gate's cost
    tests/model_provisioning.rs  a captive portal, a portal padded to the pinned length, a
                              right-size/right-magic/wrong-content body, an interrupted
                              transfer, a resume that still verifies, a full disk. NO SOCKET
                              IS OPENED: the bytes are supplied directly, which is the whole
                              reason the transport and the judgment are separate files
    examples/voice_loop.rs   the reproducible end-to-end proof (audio -> Claude -> speakers)
    examples/device_probe.rs what the audio hardware on THIS machine actually reports
    examples/noaudio_live.rs live mute/unmute check on the real device (PASS 2026-08-24)
    tests/watermark_cadence_tests.rs that recompute the rotation cadence from the
                              RAW 2026-08-28 capture on every run, both directions
    tests/phone_intake_tests.rs of THE PHONE'S ROAD IN — the `IntakeRecord::Channel`
                              record and `Spine::poll_intake`, the idle drain the phone-client
                              plan calls "not a half-day nicety any more; it is the feature".
                              Every negative test carries its positive control in the SAME
                              test: the one that matters runs the same fixture with and
                              without `poll_intake` one call apart
    tests/ended_turn_tests.rs of WHICH CONVERSATIONS' TURNS ENDED — `Spine::
                              take_ended_turns`, the one question every entrance asks at
                              its turn boundary so work he gives typed, aloud or from the
                              phone starts the same way (CEO ruling §88); each road has a
                              test, and the active thread is shown NOT to be the answer
    tests/thread_title_tests.rs tests of HAIKU NAMING A NEW CONVERSATION from his first
                              message (CEO, 2026-10-19): the whole first message is asked
                              about once, the answer reaches the shell's title sink, a later
                              message starts no second title (nor hides the first when both
                              queue at the start), and his reply never waits on it
    tests/work_gate_cost.rs   what it costs to ask "is RichOS doing anything" — timed over
                              3,000 worker rows with 1,500 liveness syscalls, because the
                              ruling the gate serves is about not getting in the way
  src-tauri/                 the Tauri shell — DETACHED nested workspace (empty [workspace])
    src/main.rs              window + Tauri command bridge to the spine
    src/window_geometry.rs   HOW BIG THE WINDOW OPENS AND WHERE, derived from the work area
                              of the display it opens on rather than from a constant
                              (docs/hardware-choices-2026-09-10.md D2 — 1400 pt on a
                              1080 pt portrait panel was 320 pt off the edge). No Tauri type
                              in it, so the arithmetic is unit-testable; the two functions
                              that talk to the runtime, read_displays and
                              remember_window_geometry, are in main.rs next to the window.
                              A saved geometry is restored only if a display present RIGHT
                              NOW holds the whole window — otherwise it is discarded whole,
                              never clamped into view
    examples/window_placement.rs  the dry run: the REAL panels via system_profiler through
                              the REAL decision, printing what the window would open at,
                              swept across menu-bar/Dock models. Opens no window
    src/nav.rs               durable rail VIEW state: width, pin, rename, archive (not evidence)
    src/output_files.rs      THE FILES A THREAD PRODUCED, REACHED SAFELY (Output side panel PRD
                              §5, slice S3): list, describe, preview, open and show in Finder by
                              OUTPUT ID only, resolved in the active thread's record and checked
                              before every read or act (no link, a regular file, the canonical
                              path it was witnessed at, O_NOFOLLOW reads); the Launch Services
                              app list; and the `richos-output://` scheme that serves media, PDF
                              and QuickLook renditions with Range, and a bad id a 404 with no bytes
    examples/output_files_probe.rs  the same commands from a command line, for the test VM's
                              open-and-reveal check until the panel has its buttons (slice S4)
    src/updates.rs           THE UPDATE PATH (RICH-TODOs rows 12 and u1): check, download with
                              throttled progress, VERIFY, install, relaunch; a nine-state
                              view emitted as `rich://update`; and the failure classifier
                              whose signature arm must never widen. The webview is granted
                              NO `plugin:updater|*` permission — these five commands are the
                              whole of its updater surface.
                              AND THE WAY BACK (2026-09-20): `rollback` fetches ONE earlier
                              tag's own immutable manifest — derived from the endpoint in
                              force, `…/releases/download/v<version>/latest.json` — through
                              the SAME signature-verifying download, and stages it with the
                              authorization startup needs to accept a downgrade. The version
                              comes from this Mac's publication history, never from the
                              server, and the plugin's default comparator (`remote > current`)
                              is why a rollback cannot happen by accident
    src/events.rs            the relay: one LiveObserver that puts §13's payloads on the
                              webview, and nothing that can widen them
    (src/phone/assets.rs)    gone since 2026-10-30, when the phone web page it embedded from
                              `richos/web/web-app/` was retired. A browser at the phone
                              address now gets a plain "Use the RichConnect app" page
                              (src/phone/routes.rs, RETIRED_PAGE).
                              A build that embeds nothing does not compile
    src/timeline_view.rs     the get_timeline command body — Timeline::view(Ceo) -> payload
    examples/timeline_payload.rs prints that payload from a real ledger (what realbytes.js
                              renders, so backend/UI field drift cannot go unnoticed)
    examples/stop_payload.rs the stop path's wire bytes, for the same reason
    examples/verify_update_signature.rs  the check a signer's exit code does not make: that
                              a produced .app.tar.gz and its .sig AGREE, under the pubkey
                              tauri.conf.json ships
    Info.plist               the macOS privacy strings (NSMicrophoneUsageDescription)
    Entitlements.plist       hardened-runtime entitlements — used ONLY by the
                              --sign developer-id path, referenced nowhere in
                              tauri.conf.json, and carried by every published release
    tauri.conf.json, capabilities/, icons/
  scripts/
    package-app.sh           the packaging entrypoint: builds, signs, notarizes,
                              staples, VERIFIES, and refuses — the caller that arms
                              build.rs's icon gate
    make-signing-csr.sh      the keypair and the CSR Apple's portal asks for; the key
                              is written outside every repository and this refuses to
                              put it in one
    install-signing-cert.sh  Apple's .cer + that key -> a usable codesigning IDENTITY,
                              verified by asking `security find-identity` rather than
                              by the import exiting 0
    rebuild-survival.sh      the acceptance test that only a SECOND install can run:
                              do the microphone and accessibility grants survive?
    generate-app-icons.sh    one artwork PNG in, every artefact tauri.conf.json declares out
    lib/app_icons.py         the generator + verifier both of the above run
    lib/no_host_paths.py     the ARTIFACT check: refuses a built bundle carrying the build
                              machine's home directory, which --remap-path-prefix cannot
                              always prevent because a macro's output is program data
    lib/no_compile_time_paths.py  the SOURCE half of the same guarantee, run by
                              package-app.sh immediately BEFORE the compile. On 2026-09-18
                              the artifact check refused a nightly after a full release
                              build over one line — env!("CARGO_MANIFEST_DIR") — that had
                              been readable in the source the whole time. Same defect, one
                              second instead of an hour. It does not replace the artifact
                              check and its own header says so
    updater-e2e.sh           THE ROW 12 DELIVERABLE: builds 0.1.0 and 0.1.1, serves a
                              manifest, makes the first BECOME the second on this machine,
                              then flips one byte and requires the install to REFUSE.
                              10/10 on 2026-08-31 — docs/verification/updater-e2e-2026-08-31/
                              CASE R (2026-09-20) adds the way BACK, on a fixture release
                              directory laid out as a published one is
                              (`releases/download/{nightly,v0.1.1,v0.1.0}/`): 0.1.1 goes back
                              to 0.1.0 through the same verified download, a tampered v0.1.0
                              archive is refused as a signature failure with the bundle
                              untouched, the boot line names both versions and the direction,
                              and from 0.1.0 there is no SECOND way back because it would be
                              a step up
    lib/updater_tar.py       the update archive, built from the SIGNED bundle (the bundler
                              makes its own from the unsigned one — see package-app.sh's
                              header) and by tarfile rather than by bsdtar, which writes
                              AppleDouble sidecars the updater would unpack into the app
    run-tests.sh             every *.test.sh here, discovered from disk, never typed. Exit 2
                              from a suite means THIS HOST CANNOT ANSWER, and it is tolerated
                              only where the caller declared that suite by name with a reason
                              — an undeclared gap, a bare name, and a declaration whose suite
                              has started answering are all red.
                              CONCURRENT since 2026-09-19, output still printed in discovery
                              order: 341 s -> 167 s on this Mac, measured at pool sizes
                              1/2/3/4/6/10 = 341/223/167/183/228/236 s. More concurrency is
                              SLOWER past three — two suites dominate and contend — so the
                              default is cores/3 floored at 2, not cores. --jobs N overrides.
                              --no-host-screen holds back every suite that boots the app
                              (those that source lib/gui-launch.sh), recording NOT RUN or
                              running it in a guest named by RICHOS_GUI_HOST; a named guest
                              with no runner REFUSES rather than using this screen.
                              RUN_TESTS_SKIP_UNCHANGED=1 skips a suite whose declared
                              `# run-tests: inputs` paths are byte-identical to a run that
                              proved it green, with any uncommitted edit counting as a change.
                              --results-out writes what ran, what was skipped and what was
                              held back, for build-info.json
    run-tests.test.sh        the harness's own allowances, held to account: 25 cases against
                              a copy of it and fake suites with known exit codes — H for the
                              host-gap declaration, P for the pool (discovery order, the
                              bound, a failure mid-pool), K for skip-when-unchanged, S for
                              --no-host-screen, whose S6 scans the REAL inventory for any
                              suite that could open a window and is neither classified nor
                              declared inert
    gui-proof-in-vm.sh       boots a candidate's OWN signed bundle in a testvm guest and
                              writes the proof `publish` demands of a --no-host-screen
                              candidate. `suite=shipped-bundle-boot`, never gui-boot.test.sh:
                              fewer assertions than that suite, and the true artifact
    nightly-local.py         the OPERATOR's release command — check / build / candidate /
                              publish / release / stable. It runs the gates, holds the lock,
                              and hands a plan to nightly.py. `gate-environment` prints the
                              ALLOWLIST a gate may see of the operator's shell: 47 exported
                              names reach 15, and a variable nobody thought about is excluded
                              by construction rather than included by construction. See
                              NIGHTLY.md
    nightly.py               the PUBLISHER — version allocation, the release commit, the
                              candidate manifest, the channel move, and the branch-ruleset
                              read-back before anything is published. Every step that runs
                              ONLY when something is released is marked `@release_step`, and
                              `release-smoke` runs all of them against a throwaway directory
                              in 0.2 s. It REFUSES if any registered step was not reached, so
                              a release-only step added and not smoked turns the build red
                              naming the function — the drift T3 Code's own smoke has
    stable-promotions.json   the record of the CEO's own decision to promote a nightly to
                              stable (§69). `stable --from-nightly` refuses without an entry
                              naming that exact tag or his words on its own command line
                              (`--ceo-words`), which it commits into this file in the stable
                              release commit itself. A READY verdict, a gui-proof and a green
                              gate are preconditions for publishing a NIGHTLY and none of
                              them promotes anything. Committed empty; Rich writes his words
    *.test.sh                fourteen at this commit, counted with `ls scripts/*.test.sh |
                              wc -l` rather than by reading this line — which said "nine"
                              until 2026-09-18, when it was three behind before the tenth
                              was even added. The list is NOT typed here for the reason
                              run-tests.sh gives, and a typed COUNT drifts the same way a
                              typed list does. Measured on a GitHub macos-latest runner,
                              2026-09-10, run 34446378461: 8 suites, 147 checks, and
                              gui-boot.test.sh declared as a gap because no public runner can
                              hold the loro compiler its fixture copies
  UPDATES.md                 how RichOS updates itself: what is proven, what is NOT proven in
                              those words, the signing key and where it may not live, the
                              manifest format, and the four hosting options — which is a CEO
                              decision and is left as one
  updater/latest.example.json  the manifest format, with a home in the repository rather
                              than only in a build directory
  ui/                        the web UI — the CEO-facing surface
    index.html               the shell: rail, header, timeline, inspector, composer
    main.js                  the WIRING — commands, the four families' listeners, the rail,
                              per-thread draft/scroll, voice, and the render loop
    timeline.js              the timeline MODEL and its DOM: the fence, the visibility gate,
                              the idempotent upsert, §6.4's two disclosure defaults
    style.css                the calm paper palette and the work grammar (§17)
    mock.js                  the dev harness — inert in the real Tauri build; the ONLY way
                              every state is reachable without a live Claude
    tests/                   the browser acceptance harness (see tests/README.md)
```

`src-tauri/` is a **deliberately detached** nested workspace so the heavy webview
dependency tree never gates `cargo test -p richos-core`. richos-core is a path
dependency, so the shell always builds against the same spine.

## Entity and thread navigation (Codex-UX slice 4)

Contract: `richos-hq/docs/design/richos-codex-inspired-conversation-ux-2026-08-28.md` §3 and §25.

**Grouping happens in Rust, not in the renderer.** `navigation_tree` returns threads
already inside their entity's group, resolved through `Ledger::thread_binding` — the
accessor that reads the immutable durable record. An entity is a privacy boundary
(§1), so a renderer that bucketed a flat list by an `entity_id` string would be one
`if` away from a boundary violation with nothing to catch it. `richos/app/ui/main.js` renders
the groups it is given and never re-sorts them and the main-pane header renders from
`active_context` — the spine's binding — so a UI bug can show the wrong thread but
cannot mislabel which entity the CEO is talking to.

**A thread with no entity home is rendered, calmly and refused.** A record written
before entity scoping replays as `ThreadEntity::Unbound`: listed under "Needs an
entity", never inside one, never activated, never read. The pane states the reason in
Rich's voice and the composer is disabled. `Ledger::adopt_unbound_thread` is the only
exit and is **not reachable from the shell** — `Spine` exposes `&Ledger`, not `&mut` —
so binding one remains a programmatic/back-office act until a core change lands.

**Rail status marks are never fabricated.** The full list, with the signal that earns
each, is `STATUS_MARKS` in `richos/app/ui/main.js`. §3.2's *Queued* and *Waiting for CEO* are
absent because no per-thread enqueue event and no waiting signal exist in this build
and §22 lists worker waiting state as something that must not be faked. No worker
count appears in the rail at all.

**Pin, rename and archive are shell state, not ledger events** (`src/nav.rs`). §25
requires them to work *without changing context authority*; the ledger is evidence and
has no rename/pin/archive event to append. A rename is therefore a display override
with the ledger title returned untouched beside it and archiving changes which list a
thread appears in and nothing else about its scope.

## Between threads and across a restart (Codex-UX slices 9 and 10)

Contract: §24's last two slices — *"verify restart, multi-thread and scope behavior"* and
*"update streaming and UI contracts"*. Slices 1–8 each proved ONE thread, in isolation, for
the length of ONE turn. These two close what that leaves open and they are two halves of
one job: the first checks the behavior, the second checks the sentences about it.

**What is verified** (`richos/app/ui/tests/restart-scope.js`, through the real shell under WebKit):

- **A working thread stays visibly active while another is selected** and returning to it
  RESUMES its timer rather than restarting it — measured, not asserted about: leave at 0s,
  return at 3s, still ticking, derived from the turn's own `startedAt`.
- **A draft and a scroll position belong to one thread.** The draft one is a privacy
  control rather than a convenience: an entity is a boundary, so a half-written sentence for
  one company sitting in another's composer is one Enter from being filed in the wrong
  place.
- **A turn streaming elsewhere renders nothing here** — across entities AND across two
  threads inside one entity, which is the half only the fence's `threadId` clause catches.
- **The fence is on EVERY live handler.** The inventory is read off the shipped
  `window.RichTimeline` and cross-checked against the number of `accepts()` call sites in
  `timeline.js` on disk, so an eighth handler that forgets the fence fails the check rather
  than quietly widening the surface.
- **Duplicates render once** and **missed events recover from the durable snapshot** —
  proven by replacing the whole typed family with functions that throw the events away and
  watching the reconciliation reload put the turn back.
- **A turn still in flight when the app closed reads "outcome unknown", never finished**
  (§14: *"Never infer that a turn completed because the app was closed"*) and **a mid-turn
  crash draws the CEO's prompt exactly once** — measured across the whole recovery window,
  not just at the end, because the end state is right either way.

**One defect it found.** §15's *"preserve each thread's scroll position during thread
switching"* did not work: the position was written while the PREVIOUS thread's DOM was
still mounted, so the browser clamped it and the render's height anchor then added the
difference between the two threads' heights. A constant off-by-121px at 1200x420 against
the seeded FemcBoost thread. Fixed; the mutation that removes the fix is M4 in the
transcript below.

**Every check was run RED once**, by breaking the thing it guards in the shipped source.
The ten runs, the coverage map derived from them and the one check with no mutation of its
own are in
[`docs/verification/restart-scope-2026-08-30/mutation-runs.txt`](../../docs/verification/restart-scope-2026-08-30/mutation-runs.txt).
Two of those runs changed the tests instead of confirming them, which is the whole argument
for doing it.

**What keeps the documents true** (`richos/app/ui/tests/docs-claims.js`). Slice 10 rewrote the
stale claims in this file, in `richos/app/STREAMING.md` and in `richos/app/ui/tests/README.md`; the suite
is what stops them going stale again. It joins three kinds of claim to the tree and types
nothing:

| Claim | Joined to |
|---|---|
| every test file named in this file, with no count beside it | the test files that exist on disk |
| the suite table in `ui/tests/README.md` | the inventory `run.js` discovers from disk |
| every `rich://` name in `STREAMING.md` | the `pub const … = "rich://…"` the Rust source declares |

Its first run is the demonstrated failure and nothing in it was staged:
[`docs/verification/restart-scope-2026-08-30/docs-claims-before-slice-10.txt`](../../docs/verification/restart-scope-2026-08-30/docs-claims-before-slice-10.txt).
Five of six checks red — two crate totals wrong, four per-file counts
stale, three whole test suites documented nowhere, three browser suites missing from their
own table and four declared `rich://` events absent from the document that calls itself
*"the whole contract the UI needs"*, `rich://proactive-message` — Rich speaking unprompted
— among them.

This is deliberately the gap `richos/engine/scripts/publication-completeness.sh` names as beyond
its reach: *"SEMANTIC honesty. Every path in a document can resolve while the sentence
around it is false."* Every path in `richos/app/README.md` did resolve the whole time.

## Machinery routing and the technical view (techy mode)

Every non-text agent frame is **routed**, not dropped, into a second event family
(`rich://machinery`) and retained in a separate per-thread journal at
`<app-data>/machinery/<thread_id>/<YYYY-MM-DD>.jsonl`. Contract:
`richos-hq/docs/plans/richos-techy-mode-2026-08-26.md`. UI contract: `richos/app/STREAMING.md`.

**Retention is unconditional and has no setting** — that is what makes it possible to turn
the technical view on for a conversation that already happened.

**The technical view itself landed 2026-08-30.** `get_machinery` serves one thread's
machinery as its timeline at `ViewMode::Technical` — the same items, the same ids and the
same `(turn, slot, sequence)` order as the calm view, with the technical half of each row
kept rather than removed. `richos/app/ui/` renders it inline, one line per tool call, with the
status each actually returned; `get_machinery_raw` fetches §2.4's raw pane on expand. The
toggle is `techy_mode` / `set_techy_scope` / `set_techy_default`: every entrance — ⌘⇧T, the
rail's switch, the settings row, the chip — asks WHERE it applies in three choices (all
companies / this company / this conversation), with the first preselected and Enter
confirming, per the CEO's §7.1 answer of 2026-09-18. `set_techy_mode(enabled: null)` hands a
pinned conversation back to the tier above it. **With it off, the conversation surface is
byte-identical to what shipped** — no chip, no chevron, no hint — and `tests/techy.js`
check 11 asserts that by comparing `#messages` innerHTML across a round trip.

Five limits, stated rather than discovered later:

1. **Retroactivity begins at the routing commit.** A thread that ran before it has no
   machinery at all and the honest state is *"No machinery was recorded for this
   conversation."* Nothing earlier is recoverable, ever. **That sentence is one of four**
   (`machinery_view.rs`): `nothing_recorded`, `not_retained` (nothing on this install),
   `unreadable` (the store is there and the OS refused it) and `recorded`. They are
   deliberately not one sentence — an unreadable store served as an empty one is the
   product lying about its own record.
2. **Readable thinking text produces nothing today, on EITHER wire.** Measured across five
   probe runs of `claude-agent-acp` 0.70.0, including one built solely to elicit it: zero
   `agent_thought_chunk` (`docs/verification/acp-emission-probe-2026-08-28.md` §4.1).
   Re-measured on the native binary 2026-08-31: **7 `thinking` blocks, every one with EMPTY
   text and a signature only** (`docs/verification/native-claude-stream-json-2026-08-31/`).
   Deleting the adapter changed nothing here, which is worth saying because it was the one
   row a reader might have hoped the move would fix. The route exists so there is no hole
   the day that changes; **no `● thinking` row is drawn**, because an always-empty
   affordance says the model is not thinking when the truth is that the agent does not say.

   Same for client-directed file IO and there the native answer is stronger than "not
   observed": the CLI was **OBSERVED NOT TO ASK** — it wrote and edited files itself and
   asked only for permission. `MachineryKind::ClientFsCall` therefore has no producer at
   all, which `machinery.rs` states at the constructor rather than leaving to be found.
3. **Between-turn updates are still dropped.** One `available_commands_update` at session
   start and one `session_info_update` after each turn ends reach no sink, because the
   client only delivers updates while a prompt is in flight. §1.5 designs the fix.
4. **The technical view is a RELOAD path, not a live technical stream.** While a turn runs
   its rows arrive from the calm live family, which is CEO-shaped by construction, so they
   gain their technical half when the turn ends. `rich://machinery` still has no UI
   subscriber and that is deliberate: subscribing would turn "the calm view does not
   subscribe to this event" from a structural fact into a runtime branch.
5. **No control of any kind.** No interrupt, no approve/deny, no re-run. Techy mode is a
   window, not a cockpit — `tests/techy.js` check 15 asserts the only button inside a
   technical row is its own disclosure.

**The four §7 questions are the CEO's and are left open** (`richos-hq/wiki/open-items.md`
1.4). Global default vs per-thread: both exist and both are reversible
(`set_techy_mode(enabled: null)` hands a pinned thread back). The raw-payload window:
nothing in the read path or the renderer consults it and no sentence names a duration.
Whether customers can find it: no conversation-surface affordance while it is off. Whether
deleting a thread deletes its machinery: no delete-thread command is added and both
primitives exist (`MachineryJournal::delete_thread`, `ConfigStore::forget_techy_thread`).

## Feedback channel — the local half (v1)

RichOS asks `How is RichOS doing this session?` with four keys — `1` Bad, `2` OK but
could be better, `3` Good, `0` Dismiss. On `1` or `2` it offers to let Rich tell the
RichOS developers, **fully anonymized and generically**, what annoyed the user and why it
happened. Before that could ever travel, the user sees exactly what would be said.

**This version has no outbound half at all.** No transport, no endpoint and deliberately
no queue for a later version to find and flush. `tests/feedback_no_outbound_tests.rs`
asserts that seven ways rather than promising it in a comment — four over the module and
three more over the Tauri commands, everything those commands call and the shipped web
layer, because a claim written when the feature had no caller stops covering it the day it
gets one.

**The taxonomy is the feature and it is a type problem rather than a filter problem.**
A filter reads free text and decides whether it is safe. `FeedbackPayload` has no `String`
field at any depth: a report is assembled from a closed, versioned vocabulary of terms
that were authored once and compiled in, so there is nowhere for the user's specifics to
sit. It is the same move as `Timeline` refusing to implement `Serialize` — the unsafe
thing does not exist, rather than being caught.

Why a filter would not have done, on the reference case this was built against: one of its
negative controls contains **no proper nouns at all** and is still disqualifying, because
it discloses what the user does for a living. It reads "generic" both to the model that
wrote it and to the human asked to approve it. Nothing that inspects prose catches that
class reliably.

What is pinned and where:

| Claim | Held by |
|---|---|
| Free text cannot enter a payload | two `compile_fail` doctests on `FeedbackPayload`, with a positive control beside them |
| Prose is refused at the JSON boundary in every smuggling shape | `negative_control_*` tests (diagnosis field, term list, condition list, failure class and an extra field) |
| The reference case's target payload IS expressible | `positive_control_the_reference_cases_target_payload_is_expressible` — without it the rejection tests could pass by rejecting everything |
| The rendered report matches the target byte for byte | `the_reference_case_target_payload_renders_exactly_this` |
| Everything the feature can say is a finite word set | all 60,960 expressible reports are rendered and every token checked against the vocabulary |
| A change to the vocabulary needs a version bump | `vocabulary_fingerprint()` pinned in a test |
| The user cannot be asked to consent to a report he was never shown | `ApprovedReport` has a private field; the only public constructor is `Disclosure::approve()` |
| ...and that survives the IPC boundary too | `feedback_record` re-renders the selection and refuses an approval whose text is not byte-identical to the block the webview says it showed |
| The browser harness cannot rehearse wording the product no longer says | `tests/feedback_surface_tests.rs` writes the fixtures `ui/tests/feedback.js` joins `mock.js` to |

Two limits, stated rather than discovered later:

1. **The prompt is the fallback, not the capture mechanism.** In the reference case all
   five moments of real annoyance were volunteered mid-work, unprompted; none arrived at
   session end. A prompt fired at a chosen moment would have caught none of them at the
   moment they were felt. Catching what is already being said is a larger, later piece.
2. **It appears when he opens it and at no other time.** The rail carries a permanent
   `Feedback` control with no count on it — nothing on that surface is ever waiting on
   him. There is no timer, no end-of-session prompt and no trigger and limit 1 above is
   the reason: a prompt fired at a moment of RichOS's choosing would have caught none of
   the five and one that arrives during the work he is annoyed about is itself an
   unprepared task handed to him. Reachable-when-he-wants-it is the honest fallback;
   catching what is already being said is still the larger, later piece.

## Booting the app in a test — it will not take the CEO's screen

**If you are about to write something that starts `richos-tauri` and then stops it, read
this paragraph and nothing else: you do not have to do anything.** Your boot will come up
with no Dock icon, no window on his monitor and no keyboard focus and it will still be a
complete, driveable app. You do not opt in, you cannot forget to and no variable has to be
set.

**The complaint, 2026-09-06, in his words:** *"It opens in the foreground on the main
monitor AND takes away focus from the current app. So, if I'm typing something here, it
takes away focus from here and focuses on that app."* Several times in a row, because a
harness proving an assignment survives a restart boots, kills and reboots the binary —
`gui-boot.test.sh` alone launches it seven times per run.

**And the shape of the fix is his second sentence, not his first:** *"GENERAL issue if and
when the same or similar tests are run by others in the future."* So RichOS does **not**
detect a test boot. Detecting tests means keeping a list of test shapes and the author of
the next harness is not on that list and never will be — the hand-maintained-inventory
failure this repository spent 2026-09-04 and 2026-09-05 taking out of five other checks.
It is inverted instead:

> RichOS activates, takes the keyboard and appears in the Dock **only when it can
> positively establish that it is an installed launch by the person who installed it.**
> Everything else is accessory.

Three facts, all required and none of them names a test — `richos/app/src-tauri/src/activation.rs`
argues each one:

| | what has to be true | what it catches |
|---|---|---|
| **B** | the executable is inside a `.app` whose `Info.plist` declares this build's identifier | a `cargo build` binary; a loose copy of the executable |
| **D** | the directory this boot writes to is `$HOME/Library/Application Support/<identifier>` | any redirected data directory — a scratch `HOME`, a `tempfile.mkdtemp`, an override variable this code has never heard of |
| **P** | `getppid()` is 1 | anything a program started, because a harness has to **hold** the process it boots: it needs the pid to wait on it, read it and kill it. macOS hands a real launch to launchd |

`P` is the one that reaches the hardest case, which the other two do not: a harness that
boots the real installed bundle against the real data directory. The same argument is made
one more time over a different object, in the private orchestration engine's worktree
retirement library: it identifies a workspace from the ownership record and refuses anything
it cannot derive, rather than listing the shapes it knows how to delete.

**Every boot says which answer it got and why**, on its first line, which is where you will
actually meet this:

```
[richos] activation: accessory — no Dock icon, no window on screen, no focus taken, because
this is not an installed launch: …/target/debug/richos-tauri is not inside a .app bundle; a
program is holding this process (parent pid 32883), and macOS hands a launch to launchd
(pid 1). The window is still real and still driveable; call show() on it, or set
RICHOS_ACTIVATION=regular for the whole normal treatment.
```

**If your test genuinely needs the window on screen**, call `show()` on it or set
`RICHOS_ACTIVATION=regular` and accept that the run will take his keyboard. The variable
answers in both directions (`regular` / `accessory`) and an unrecognized value decides
nothing, so a typo cannot hand a test run the screen.

**Two things worth knowing before you try to improve this.** The obvious fix —
`ActivationPolicy::Accessory` plus a window built `.focused(false)` — **does not work** and
was measured not working: `lsappinfo front` still reported `richos-tauri` frontmost for 22
of 24 samples. tao sends `makeKeyAndOrderFront:` to every *visible* window at
`applicationDidFinishLaunching` and calls `activateIgnoringOtherApps(true)` unconditionally
and neither consults the policy or the requested focus. What works is building the window
**invisible** (tao skips invisible windows by name) plus the policy (which is what keeps the
Dock clean — ablated separately and both halves earn their line). Measurements and the tao
citations are in `main.rs`'s `set_activation_policy` block and in
`docs/verification/activation-2026-09-06/`.

## Build & test

```sh
# 1. The spine — fast, no native deps, no network:
cargo test -p richos-core                       # unit, integration and doc-tests; cargo reports its own counts
# Summarize a captured log separately: python3 scripts/rust-test-summary.py /path/to/cargo.log
# Ordinary passes and doc-test passes are separate; do not add them together.
#     ONE IGNORED CHECK NEEDS A REAL LORO CORPUS, which is the CEO's own record, lives outside
#     every repository and is on no clean checkout. `home_field::tests::
#     against_a_real_corpus_when_one_is_configured` compiles one into the home screen's
#     picture and prints its counts; run it by hand with
#     `RICHOS_LORO_DIR=<checkout>/loro LORO_ROOT=<checkout> cargo test -p richos-core --lib
#     home_field -- --ignored --nocapture`. It is `#[ignore]` rather than an early `return`
#     for the reason the two below are: a test that returns is reported `ok`.

#     ONE OF THEM NEEDS A `claude` ON A MAC, and that is deliberate as of 2026-09-05.
#     `the_real_claude_binary_on_this_machine_satisfies_the_requirement` is the POSITIVE half
#     of the Anthropic signature pin; it used to `return` when the binary was absent, and a
#     test that returns is reported `ok`. It now FAILS instead, naming every path it searched
#     — `$RICHOS_CLAUDE_BIN` points it at a binary anywhere. Off macOS it reports
#     `ignored, NOT CHECKABLE ON THIS TARGET: …` (the pin is read by `/usr/bin/codesign`),
#     which is what `app-spine-ci.yml`'s ubuntu runner now prints in place of a green line.

#     AND ONE IS A RELEASE GATE, added 2026-09-06. `tests/doctrine_sentinel.rs` costs a real
#     API turn against the customer's own subscription, so it cannot be in the default run —
#     but `--append-system-prompt-file` is semi-documented and the binary self-updates, so
#     "does RichOS's standing instruction still reach the model?" has to be re-asked against
#     the binary each release ships against, and the answer recorded with that version:
#         cargo test -p richos-core --test doctrine_sentinel -- --ignored --nocapture
#     Last run: PASS against 2.1.263 (docs/verification/inner-doctrine-live-2026-09-06/).

# 1b. Voice mode — pure logic + the native edges (no mic needed):
cargo test -p richos-voice                      # live-device tests are ignored without a device
#     The library tests include four `ignored, LIVE AUDIO: …` tests, each naming its own
#     reason. Those four open a real output device and one is audible for about a second, so
#     they are opt-in. Until 2026-09-05 they opted out with an early `return` — and a test
#     that returns is reported `ok`, so they were four green lines asserting nothing on every
#     machine but the CEO's and on every CI run. `crates/richos-voice/build.rs` turns the
#     variable below into `cfg(live_audio)` so the default run says `ignored` instead.
RICHOS_VOICE_LIVE_AUDIO=1 cargo test -p richos-voice   # all run, incl. the audible ones
cargo run -p richos-voice --example device_probe       # what the audio hardware really is

# 2. The desktop shell (from richos/app/src-tauri/):
cargo build                                     # -> target/debug/richos-tauri (Mach-O)

# 2b. A SHIPPING build must carry the ENGINE PIN, or first-run setup refuses to install an
#     engine (SetupError::EngineUnpinned) rather than fetching whatever the URL returns.
#     `make-engine-asset.sh` builds the release asset deterministically, proves it is
#     deterministic by building it twice, and prints the three values:
richos/app/scripts/make-engine-asset.sh --check
source richos/app/target/engine-asset/engine-pin.env   # RICHOS_ENGINE_{VERSION,URL,SHA256}
#     Upload the asset to the public repository's Releases BEFORE shipping a build that
#     carries the pin — a pin is a claim about bytes that must already be there, and a
#     build that ships first gives every customer a 404 and a named DownloadFailed.
#     `crates/richos-core/build.rs` declares rerun-if-env-changed for all three, so
#     changing the pin is a rebuild and not a cache hit carrying the old digest.

# 3. The LIVE round-trip. Needs Claude Code installed and signed in — and NOTHING else:
#    no npm, no Node, no adapter. $RICHOS_CLAUDE_BIN overrides the binary; by default
#    ~/.local/bin/claude is preferred over PATH.
cargo run -p richos-core --example native_roundtrip -- "$PWD/../engine" "who are you?"

# 3a. THE LAUNCH THE CEO PERFORMS, end to end. A double-clicked bundle has working
#     directory `/`, which owns no entity, so the send is refused; this runs the whole
#     sequence — refused, answered, remembered, reopened, and the same sentence landing
#     with a real lease. The four steps that need no compute are also tests.
cargo run -p richos-core --example company_choice_roundtrip -- "$PWD/../engine" richos

# 3b. The LIVE machinery proof — the same chain, but showing that tool calls are ROUTED
#     and RETAINED: the calm view, the interleaved (turn, seq) stream, the merged rows,
#     and the journal files on disk. Leaves the journal in place and prints its path.
cargo run -p richos-core --example machinery_roundtrip -- "$PWD/../engine"

# 4. The WHOLE voice loop (a WAV stands in for the mic on a machine with no input
#    device):
say -v Samantha -o /tmp/ceo.wav --data-format=LEI16@16000 "Rich, are you there?"
cargo run -p richos-voice --example voice_loop -- /tmp/ceo.wav
```

```sh
# 5. The UI, in the engine Tauri actually renders through (from richos/app/ui/tests/):
npm install && npm test                         # every suite, discovered from disk
node restart-scope.js                           # one suite, while you are working on it
```

**Cargo's final output and local build identities belong to each physical workspace.** A shared target can
run another checkout's stale binary when its source timestamps are older than the cached
artifact. `scripts/bin/cargo` prevents that by treating `CARGO_TARGET_DIR` and
`--target-dir` as cache roots and deriving a separate output directory for each workspace.
The test runner, lint entry point and RichOS CPU admission wrapper enable this
dispatcher. For a direct development command, use `scripts/bin/cargo test` or source
`scripts/lib/cargo-cache-env.sh` before using `cargo`. Explicit output-directory overrides
in Cargo config must be removed; choose a cache root through `CARGO_TARGET_DIR` instead.

`bash .richos/worktree-setup` links default `target/` paths to those private directories
on the external SSD. `scripts/shared-build-cache.sh --check` detects old shared links;
run the script without `--check` to retarget the links while preserving their old cache.
Cargo 1.95.0 builds compatible dependencies once in `<cache-root>/shared-build-v1`.
Cargo retains its build lock, parallelism and input validation, including features,
flags, compiler identity, build scripts and generated sources. Each workspace gets a
distinct `RUSTC_WORKSPACE_WRAPPER` path, which Cargo hashes into its local units;
nonmembers outside the workspace already have absolute source identities. Final
binaries and libraries stay in the private target directory. Cargo's internal
`__CARGO_DEFAULT_LIB_METADATA` switch forces hashed intermediate filenames for
cdylibs too. The A/B/A regression loads the libraries, not just test executables.

This behavior is qualified for Cargo 1.95.0. Other versions, custom workspace
wrappers, included configuration and excluded local dependencies inside the workspace
retain fully private build directories. A toolchain upgrade needs the reuse and
isolation tests before extending that qualification. sccache 0.18.0 is bypassed for
shared-build commands because it cannot identify this workspace wrapper; other
compiler wrappers are preserved. Private fallback commands keep existing cache setup.
Registry downloads remain shared in both modes. Dependencies with changed effective
inputs are rebuilt by Cargo, even when their lockfile entry is unchanged.
See [Cargo's build-cache documentation](https://doc.rust-lang.org/cargo/reference/build-cache.html).

Playwright remains shared: `ui/tests/lib/harness.js` derives the main checkout's install
when this directory has no `node_modules`. The nightly release checkout keeps its separate
setup. Never copy compiled Cargo output between checkouts.

Contract and rules: [`richos/app/ui/tests/README.md`](ui/tests/README.md). Two notes that belong
here rather than there:

- **This file states no test count and that is checked.** Cargo prints its own counts. The
  totals this file used to carry went stale by a wide margin (`97/97` against 304, `121/121`
  against 163) while every sentence around them stayed true, which is the one thing
  `richos/engine/scripts/publication-completeness.sh` explicitly cannot catch: *"Every path in
  a document can resolve while the sentence around it is false."* Checking them kept them true
  but made every branch that added a test edit the same line and two such branches conflicted
  on nothing but the number (2026-10-19). So `richos/app/ui/tests/docs-claims.js` now checks that
  every test file is named and that no count is written beside one.
- **CI covers the spine and the browser suites and neither has ever run.**
  `.github/workflows/app-spine-ci.yml` runs `cargo test -p richos-core` and a release
  build; `.github/workflows/ui-suite-ci.yml` runs every suite under `richos/app/ui/tests` on
  macOS, under the WebKit that ships in its own `node_modules`. The sentence that stood
  here until 2026-09-04 — *"the browser suites have no runner at all"* — was written
  before that second file existed and is simply out of date. What is still true and is
  the part that matters, is that **neither workflow has ever executed**: both were
  authored on the machine that runs the suites locally and every workflow in this
  repository was switched off on 2026-09-01 over a billing block. So neither may be
  called CI-verified until a green run exists on a SHA somebody can name. Both were run
  verbatim from a fresh clone on 2026-09-04 —
  `docs/verification/workflows-on-a-clone-2026-09-04/` — which answers a narrower
  question: that they do not depend on the author's machine.

## App icon — generated from real source art, verified and shipping

**CEO handover sheet: [`richos/app/APP-ICON.md`](APP-ICON.md)** — what the icon is, where to
look at it and the one command that replaces it. Nothing is asked of him there any more;
this section is the engineering detail behind it.

`src-tauri/icons/` holds all six files `tauri.conf.json`'s `bundle.icon` declares, each a
distinct real image: `32x32.png` (32x32), `128x128.png` (128x128), `128x128@2x.png`
(256x256), `icon.png` (512x512), `icon.icns` and `icon.ico` carrying six layers from 16px
to 256px. They were produced by the pipeline below from
`richos/app/icon-source/richos-icon-1024.png` (1024x1024 RGBA, with the vector original beside
it), landed 2026-08-31. The verifier agrees:

```
$ python3 richos/app/scripts/lib/app_icons.py verify
OK: every artefact declared in tauri.conf.json bundle.icon is present, correctly sized, and distinct.
```

That set is what customers have. `Contents/Resources/icon.icns` inside the published
`RichOS.app` of 1.0.2 is byte-identical to the file in this tree — SHA-256
`e4f98a1f58e18514cb4521523df2f278a16fa96255bc7931d21fd460abe9f2f0` on both.

### Generating the real set

```sh
richos/app/scripts/generate-app-icons.sh /path/to/artwork.png
```

One square PNG of at least 1024x1024 with a transparent background in; every artefact
`tauri.conf.json` declares out, verified before the command exits. Source requirements
are enforced, not documented: wrong size, missing alpha, non-square, JPEG, blank and
full-bleed artwork are each rejected by name, all problems reported at once.

To re-check an existing set without regenerating:

```sh
python3 richos/app/scripts/lib/app_icons.py verify
```

### Derived, never typed

Both the generator (`richos/app/scripts/lib/app_icons.py`) and the build gate
(`src-tauri/build.rs`) derive the required artefact list from **`tauri.conf.json`'s own
`bundle.icon` array**. Neither contains a hand-typed list of filenames. Adding a size to
that array is picked up by both with no code change — proven by adding `icons/64x64.png`
and watching the gate warn and the generator emit it. An entry that cannot be decoded is
a hard failure, never a skip.

That rewrite fixed a real hole: the previous gate hand-typed four filenames while the
config declared six, so `icon.icns` and `icon.ico` — the only artefacts that carry the
icon on macOS and Windows — were never checked on any build.

### Warn on build, fatal on bundle

Deliberate and load-bearing: app development must never be blocked on artwork nobody
has yet.

- `cargo check` / `cargo build` → **warnings only**, exit 0.
- `RICHOS_REQUIRE_REAL_ICONS=1` → **panics**, refuses to build.

**`richos/app/scripts/package-app.sh` sets it** (2026-08-30). Two earlier versions of this
paragraph were wrong in opposite directions: one claimed the variable was "set by
bundling and CI" when nothing set it and its correction then said
`.github/workflows/` contained only `engine-self-verify.yml`. Its correction's
correction said three. **There are five** — `app-spine-ci.yml`,
`engine-self-verify.yml`, `packaging-ci.yml`, `ui-suite-ci.yml` and
`windows-companion-ci.yml` — and `.github/workflows/README.md` is where their states
are kept, because a count in a paragraph has now been wrong three times running.
The **conclusion** that sentence was drawn to support is nonetheless still true and
survives the correction: **no CI job builds this crate.** `app-spine-ci.yml`'s own
header excludes `richos/app/src-tauri` by name as "a deliberately detached workspace with the
whole webview dependency tree behind it". So CI is not what arms the gate and an
ordinary `cargo build` must stay non-fatal.

The packaging entrypoint is what arms it and the pre-flight it runs first is
convenience rather than the guarantee. Proven by deleting the pre-flight from the
script and resizing `icons/32x32.png` to 16x16: the run still stopped at `build.rs`'s
panic and exited 4 with nothing packaged.

Both modes were measured while the committed set was still placeholders: 12 warnings and
`Finished dev profile` in the first mode, a hard panic in the second. The committed set is
now the real one, so strict mode compiles clean with zero icon warnings — which is the mode
every released build is compiled in.

### Tooling and license

**Pillow, SPDX `MIT-CMU`** (read from the installed distribution's `License-Expression`
metadata, Pillow 12.3.0) for decode/resample/PNG/ICO, plus Apple's own
**`/usr/bin/iconutil`** for the macOS `.icns`. Both are **authoring-time only**: nothing
from either is linked into or shipped inside the signed `.app` and the only artefacts
that ship are pixels derived from the supplied artwork. That keeps the license question
entirely clear of the signing/notarization path.

### What Tauri actually requires

(Tauri v2.11.5 desktop app / tauri-build 2.6.3, pinned `"2"` in `Cargo.toml`.)

- `tauri.conf.json`'s `bundle.icon` array is the source of truth and lists all six:
  `icons/32x32.png`, `icons/128x128.png`, `icons/128x128@2x.png` (256x256px — the "@2x"
  of 128), `icons/icon.png` (512x512px), `icons/icon.icns`, `icons/icon.ico`
  ([App Icons](https://v2.tauri.app/develop/icons/)).
- `icon.ico` (Windows) carries layers for 16, 24, 32, 48, 64 and 256px
  ([Tauri v1 Icons guide](https://v1.tauri.app/v1/guides/features/icons/), format
  unchanged in v2). Verified: `cargo tauri icon` 2.11.4 emits exactly that set.
- `icon.icns` (macOS) must cover pixel sizes **16, 32, 64, 128, 256, 512, 1024**.

  **Correction to what this section previously claimed.** It said the `.icns` "must
  contain the 10 named/sized layers" of Tauri's `helpers/icns.json`, including `is32`
  and `il32`. That describes what one generator emits, not what macOS requires. Measured
  here: `cargo tauri icon` 2.11.4 writes `is32`/`il32` (the pre-10.7 24-bit RLE variants
  with separate `s8mk`/`l8mk` masks) for 16/32px, while **Apple's own `iconutil` writes
  the ARGB `ic04`/`ic05` instead** and never emits `is32`/`il32` at all. Both cover the
  identical set of pixel sizes. So the checks require **pixel-size coverage, not a
  literal fourcc list** — requiring the fourccs would reject output from Apple's own
  tool, which would be a checker bug rather than an artwork problem.
- The 1024px `ic10` layer is why the source must be at least 1024x1024: anything smaller
  upscales for exactly the layer shown largest.

Coverage was verified against the authoritative tool rather than assumed. Installed
tauri-cli 2.11.4 and ran `cargo tauri icon` on a synthetic 1024x1024 source:

```
required ICNS sizes     : [16, 32, 64, 128, 256, 512, 1024]
cargo tauri icon 2.11.4 : [16, 32, 64, 128, 256, 512, 1024]
this pipeline (iconutil): [16, 32, 64, 128, 256, 512, 1024]
required ICO layers     : [16, 24, 32, 48, 64, 256]
cargo tauri icon 2.11.4 : [16, 24, 32, 48, 64, 256]
this pipeline (Pillow)  : [16, 24, 32, 48, 64, 256]
```

The verifier also accepts `cargo tauri icon`'s own unmodified output (exit 0), so it is
not over-fitted to this generator.

### Named degradation: off macOS

There is no `iconutil` outside macOS, so `.icns` falls back to Pillow's ICNS writer,
which emits **no 16px layer** (measured: `ic07 ic08 ic09 ic10 ic11 ic12 ic13 ic14`). The
fallback prints a warning saying exactly that and telling the caller to regenerate on
macOS before signing a release. It does not pretend to be equivalent and the verifier
fails it — `icons/icon.icns is missing layer size(s) [16]px`.

### Fixed along the way

The `@2x` icon was literally named `[email protected]` on disk — Cloudflare
email-obfuscation mangling — and `tauri.conf.json` carried the same mangled string, so
config and file agreed with each other and nothing noticed. Every standard generator
emits `128x128@2x.png`, so the first real generation run would have written a file the
config was not looking for. Fixed in `ad017b8`; the mangled form is now rejected by the
derivation as an unsupported extension, so it cannot recur silently.

## Packaging — one command, one line and it refuses

```sh
richos/app/scripts/package-app.sh                       # ad-hoc signed; the default, and all a machine with no certificate can do
richos/app/scripts/package-app.sh --sign developer-id   # discovers the identity; needs a certificate
RICHOS_NOTARIZE=1 richos/app/scripts/package-app.sh --sign developer-id   # ...and notarizes and staples
richos/app/scripts/package-app.sh --sign developer-id --dry-run           # resolve everything, build nothing
richos/app/scripts/package-app.sh --verify-only path/to/RichOS.app [--expect-notarized]
richos/app/scripts/package-app.sh --updater             # ...and the signed update artifacts
```

The last line is the whole report. In the default mode it names what it did not do and
the cdhash in it belongs to that one run — it moves with the shipped bytes:

```
OK: RichOS.app is bundled, ad-hoc signed and verified — real icons, cdhash
fc5051ac9ca8e4713ccaec6a7e02dd3b3efb16ce, microphone usage string present; NOT
notarized, Gatekeeper rejects it, and its permission grants die on the next build
that changes a shipped byte. Path: …/release/bundle/macos/RichOS.app
```

The `--sign developer-id` modes print the same shape with the opposite content —
`package-app.sh:1279` is the notarized-and-stapled line, `:1287` the signed-but-not-
notarized one. That is the path v1.0.0, v1.0.1 and v1.0.2 were built through and what
those published bytes carry is measured further down rather than asserted here.

Anything else is a refusal that names its reason. Exit codes: `0` success, `1` the
bundle failed verification, `2` refused before building, `3` a prerequisite is
missing, `4` the build itself failed, `5` the bundle is good and the update
artifacts are not.

`--updater` adds `RichOS.app.tar.gz` and its minisign `.sig`, both built from the
bundle that was just VERIFIED rather than from the builder's intermediate — the
bundler makes its own copy before anything has signed the `.app` and that copy is
deleted on every run. It needs `TAURI_SIGNING_PRIVATE_KEY_PATH` (refused inside a
git worktree, refused world-readable) and writes `latest.json` only when
`RICHOS_UPDATE_BASE_URL` says where the archive will live. **`richos/app/UPDATES.md` is the
whole picture**, including the end-to-end run that applied an update on this machine
and the two bad ones it refused.

### What it refuses on

- **Placeholder icons.** It exports `RICHOS_REQUIRE_REAL_ICONS=1`, which is what makes
  `build.rs`'s gate fatal and it pre-checks the same set so the failure arrives in two
  seconds rather than after a release compile. Against the placeholder set this
  repository used to carry it refused with eleven named problems and exited 2; against
  the real set committed now the pre-flight passes, which is what lets a bundle be built
  at all.
- **A signing configuration that cannot work.** `--sign developer-id` DISCOVERS the
  identity when the machine has exactly one "Developer ID Application"; with none or
  with more than one, it refuses and names them, because choosing between two signing
  certificates by sort order is not a thing a script may do. `RICHOS_SIGNING_IDENTITY`
  still pins one and is still checked against `security find-identity -v -p
  codesigning`. **There is no silent downgrade to ad-hoc**, because a downgrade
  produces exactly the failure Developer ID was asked for. The path still selects
  itself never: an identity appearing on the machine does not change what a plain run
  does.
- **`RICHOS_NOTARIZE=1` without notary credentials.** Either an App Store Connect API
  key (`RICHOS_NOTARY_KEY` / `_KEY_ID` / `_ISSUER`) or `RICHOS_NOTARY_PROFILE`. **There
  is deliberately no `APPLE_PASSWORD` path** — an app-specific password in an
  environment variable is readable by every process the build spawns and lands in shell
  history; the same password goes into a keychain profile once and `RICHOS_NOTARY_PROFILE`
  reaches it. The `.p8` is also refused if it sits inside a git worktree (Apple permits
  exactly one download of it) or is readable by other users of the Mac.
- **A bundle that does not verify.** Checked on the artefact, not on the builder's exit
  code: the executable named by `CFBundleExecutable` exists, `codesign --verify --deep
  --strict` passes, the signature is the KIND that was asked for,
  `Contents/Resources/icon.icns` is byte-identical to the icon this repository
  generated and `Contents/Info.plist` carries `NSMicrophoneUsageDescription`.
  `--verify-only` runs that same set against an existing bundle, so the two paths
  cannot drift apart.
- **A Developer ID signature whose DESIGNATED REQUIREMENT is a cdhash.** This is the
  only property the certificate is bought for: macOS stores `identifier
  "com.richos.app" and anchor apple generic and certificate leaf[subject.OU] =
  "<TEAMID>"` against every permission it grants and every future build satisfies it.
  A signature can be a valid Developer ID signature and still carry a cdhash-shaped
  requirement — ad-hoc's failure wearing a certificate — so the requirement is read
  back and joined to the bundle identifier on disk. Under `--expect-notarized` the
  stapled ticket is validated too, because "notarized" without a ticket means the app
  must reach Apple over the network to be admitted.

### Two answers this produced, both previously unverified

`src-tauri/Info.plist`'s own comment asked "the packaging engineer (next round)" to
check two things. Both are now measured, on a real bundle:

- **Tauri's plist merge does land `NSMicrophoneUsageDescription` in
  `RichOS.app/Contents/Info.plist`.** Confirmed with `plutil -p` on the built bundle;
  it is now a permanent check rather than a one-off observation.
- **The hardened-runtime entitlement it also asks about** is
  `src-tauri/Entitlements.plist`, applied by the developer-id path as a config overlay
  only. It has now signed: `codesign -d --entitlements -` on the published 1.0.2 bundle
  reads back `com.apple.security.device.audio-input` and nothing else, which is the whole
  list that file carries.

### Ad-hoc is not what the bundler does. It is what the bundler skips.

The first bundle this script ever produced carried
`flags=0x20002(adhoc,linker-signed)`, `Sealed Resources=none`,
`Identifier=richos_tauri-6a5998b21aa29388` and `codesign --verify` **rejected it**:
*"code has no resources but signature indicates they must be present"*. That is the
arm64 linker's automatic ad-hoc signature on the executable, with the bundle around it
unsigned — strictly worse than an ad-hoc-signed bundle, because nothing binds
`Info.plist` or the icon to it and the signed identity is a mangled crate name rather
than `com.richos.app`.

So the script signs ad-hoc deliberately and verifying `Signature=adhoc` is not
enough — that string was true in the broken state too. It also refuses
`linker-signed`, `Sealed Resources=none` and a signature identifier that differs from
`CFBundleIdentifier`.

### What ad-hoc costs, measured rather than repeated

After signing it prints the designated requirement macOS will store against every
permission grant:

```
cdhash H"fc5051ac9ca8e4713ccaec6a7e02dd3b3efb16ce"
```

A hash of that build and nothing else — no bundle identifier, no team, nothing a later
build can satisfy. It also prints Gatekeeper's own verdict (`spctl -a -vv` →
`rejected`) rather than inferring it.

**The common phrasing "every rebuild is a different application" is too strong and
this is the correction.** What moves the cdhash is the produced BYTES, not the act of
rebuilding. Measured 2026-08-30: three consecutive runs over an unchanged tree — plus
one whose only edit was a `const` the optimizer removed — all produced
`fc5051ac9ca8e…`, i.e. the same application, grant intact. Changing one shipped string
literal moved it to `27a561effd404…`. It is not a tax on rebuilding; it is a tax on
every change worth shipping. That is still the whole argument for Developer ID and it
is why the number in this paragraph is a measurement and not a warning.

### The Developer ID path is finished and it has signed, notarized and stapled a public release

The CEO enrolled in the Apple Developer Program on **2026-08-31**, so decision 1.1 is
closed (`richos-hq/wiki/packaging-and-signing.md`) and the certificate has existed on
this machine since 2026-09-01. `security find-identity -v -p codesigning` reports
`1 valid identities found` — *Developer ID Application: Alex Booster (TZ33A4QCZJ)*, SHA-1
`BF4D68E6F858688FDAD63148BD271FCA2D02474F`.

**Finished.** Identity discovery; the hardened runtime; an explicit re-sign carrying
`--timestamp`; the entitlements overlay; submission to the notary, waiting on it,
fetching Apple's own log on rejection, stapling and validating the staple; and a
designated-requirement check that is the only thing the certificate is bought for.

**Three facts follow and they are kept apart because each one can be true while the next
is not.** Each was measured on 2026-09-04 against the `RichOS.app` inside
`RichOS-1.0.2-macos-aarch64.zip`, downloaded from its public URL rather than read out of
this machine's build directory:

- **It is signed by the real identity and carries the requirement the certificate is
  bought for.** `codesign -dvv` reads back `Authority=Developer ID Application: Alex
  Booster (TZ33A4QCZJ)`, `flags=0x10000(runtime)`, `Timestamp=4 Sep 2026 at 6:09:03 PM`;
  `codesign -d -r-` gives `identifier "com.richos.app" and anchor apple generic and …
  certificate leaf[subject.OU] = TZ33A4QCZJ`, with no `cdhash` term anywhere in it. That
  is the whole point of the certificate: the grants are bound to the identifier and the
  team, not to the build.
- **It is notarized and the ticket is stapled.** `codesign -dvv` prints
  `Notarization Ticket=stapled` and `xcrun stapler validate` answers *"The validate
  action worked!"* — which is the part that matters offline, since a stapled copy is
  admitted without the Mac reaching Apple over the network.
- **A stranger can install it.** Three public releases exist, all on 2026-09-04 —
  v1.0.0 (14:19 UTC), v1.0.1 (16:00 UTC) and v1.0.2 (17:08 UTC) —
  at <https://github.com/WebDevBooster/richos/releases>. All three first-install zips were
  downloaded and checked, not just the newest: each carries the same Developer ID
  authority and a stapled ticket and on each `spctl -a -vv -t exec` answers `accepted`,
  `source=Notarized Developer ID`. That verdict was taken on a downloaded bundle rather
  than on a local build, which carries no quarantine attribute and proves nothing.

**Still true and nothing above should be read as claiming otherwise.**

- **Rebuild survival is proven at Layer 1 and no higher.** Two Developer ID signed builds
  differing by one shipped string literal produced different code hashes and
  character-for-character identical designated requirements, so the mechanism holds
  (`docs/verification/rebuild-survival-2026-09-01.md`). Layer 2 — a real permission grant
  surviving a second install — and Layer 3 — no dialog appearing on that second install —
  have no recorded run. Both need one install-and-grant round done by hand and neither
  needs anything bought or waited for.
- The path still **never selects itself** — `sign_mode` defaults to `adhoc`
  (`package-app.sh:203`), so an identity being present on the machine does not change what
  a plain run does; it only makes the run say a signed build is now possible.

**Two measured facts about the bundler, which is why this script does more than it
looks like it should.** `tauri-bundler` 2.9.4 (`bundle/macos/app.rs:135-148`)
notarizes on its own when it can resolve `APPLE_*` credentials and, on every credential
error but one, logs `skipping app notarization` and continues — so a typo produced an
un-notarized bundle from a run that exited 0. And `tauri-macos-sign` 2.3.4
(`keychain.rs:221-226`) never passes `--timestamp`, whose unspecified default
`codesign`'s own man page describes as "may result in some but not all code signatures
being timestamped" — and a secure timestamp is a notarization prerequisite. So this
script unsets those variables before invoking the bundler, re-signs with the full
explicit argv and owns the notary step itself.

### Named gaps

- **Bundle identifier.** Tauri warns on every build that `com.richos.app` ends in
  `.app`, which it does not recommend. It is left as it is deliberately — the identifier
  is pinned and must never change (`packaging-and-signing.md`, "What RichOS
  specifically needs" item 3). The window for reconsidering it has closed: the identifier
  is inside the designated requirement of three published releases, so changing it now
  would abandon the permission grants every installed copy holds.
- **No sidecars are bundled.** `whisper.cpp` and
  `ffmpeg` are not in the `.app`; the shipped 1.0.2 bundle is 19 MB (`du -sk` reports
  19,444 KB on the extracted copy) and assumes them on the host.
  When they land, each must be signed with the same identity and sealed in or the
  bundle seal breaks and takes the grants with it.
- **`--bundles app` by default**, not the `"targets": "all"` in `tauri.conf.json`. The
  `.app` is the artefact that matters; DMG creation is a separate, flakier downstream
  step whose failure says nothing about whether the application is correct. Pass
  `--bundles app,dmg` for the installer.
- **macOS only.** It refuses on any other platform with the reason. No Windows bundle
  has ever been built and there is no Windows signing certificate. Every published
  release asset is `macos-aarch64`: no Intel build has been released.
- **The auto-update channel is live, so this entry is a status and not a gap.**
  `tauri.conf.json` carries the
  endpoint `https://github.com/WebDevBooster/richos/releases/latest/download/latest.json`
  (since the GitHub move work, after the same path at `RichSpark/RichOS`, which is asked first;
  `UPDATES.md`) and the minisign public key and v1.0.2 published `RichOS.app.tar.gz`, its `.sig` and
  `latest.json` beside the first-install zip. `richos/app/UPDATES.md` is the design and the
  end-to-end run — read it knowing it was written before the certificate existed and
  still describes the ad-hoc world.
- **Launched and talked to.** As of 2026-09-01 the bundle is installed at
  `~/Applications/RichOS.app`, opened with `/usr/bin/open` (working directory `/`, launchd's
  environment), asked which company it is for, answered and given a sentence that came back
  as a real reply in 5,868 ms. Boot line, ledger and the verbatim exchange:
  `docs/verification/installed-app-2026-09-01/`. That copy was the ad-hoc build, which
  `spctl` rejects — it opened only because a locally installed bundle carries no quarantine
  attribute. The published 1.0.2 bundle is the opposite case and is measured above:
  notarized, stapled and `accepted` by `spctl` after being downloaded.

## Runtime config (env)

- `RICHOS_ENGINE_DIR` — the engine repo used as the child's `cwd` (persona + hooks).
  Defaults to the `richos/engine/` sibling of `richos/app/`.
- `RICHOS_CLAUDE_BIN` — path to the `claude` binary. Defaults to `~/.local/bin/claude`
  (Anthropic's own launcher, which SURVIVES auto-update — §16's free way to pin the
  undocumented `--permission-prompt-tool stdio` flag without modifying any binary), else
  the bare name on PATH.
- `RICHOS_CLAUDE_DEBUG` — if set, the child's stderr is echoed (developer machinery only;
  never reaches the CEO view).

### Company memory (loro) — off unless configured and never inferred

Tier C of the re-prime payload, plus the correction desk. **All three are explicit; there
is no default and there cannot be one.** A default corpus root means one owner's Rich
answering out of another's memory and exiting 0 either way, which is a larger failure than
an error wearing a success code — so with these unset the app boots, says so on stderr and
every re-prime states that company memory was NOT consulted rather than implying there is
none.

- `LORO_CORPUS` — a provisioned corpus root (`ceo/` + `companies/<id>/`), **or**
- `LORO_ROOT` — an in-repo dogfood root (a checkout with `wiki/` + `loro/`). `LORO_CORPUS`
  wins if both are set.
- `RICHOS_LORO_DIR` — the loro checkout holding `bin/loro-context.mjs` and
  `bin/loro-write.mjs`. Deliberately **not** derived from this checkout: RichOS ships no
  `loro/` directory and never will — the corpus and the vocabulary are the owner's and live
  outside a repository that gets published. When a corpus root has been resolved and this
  is unset, the root's own `loro/` directory is used if it holds both entry points — which
  is where it is by definition in the in-repo shape and where a provisioned corpus may or
  may not have one. Unusable is an ERROR naming the root, never a silent "no corpus".
- `RICHOS_NODE_BIN` — which `node` runs the compiler. Resolved the way `RICHOS_CLAUDE_BIN`
  is: explicit first, then the first `node` on `PATH` returned as an absolute path, then
  `/opt/homebrew/bin/node` and `/usr/local/bin/node`, then the bare name. **A GUI launch's
  `PATH` is `/usr/bin:/bin:/usr/sbin:/sbin`**, which holds no Homebrew `node`, so without
  this step a resolved corpus fails once per rotation with `could not start the loro
  compiler: No such file or directory`.

#### Two per-user pointers, for a launch that has no environment

Everything above is an environment variable and **LaunchServices gives a double-clicked
`.app` launchd's environment**, which has none of them. That is the same premise failure
`engine.rs` fixed for the engine directory and until 2026-09-01 the corpus still had it:
the signed bundle booted `loro Tier C: no corpus configured` with the whole corpus one
symlink away (`docs/verification/installed-app-2026-09-01/`).

So after the explicit variables — which stay **exclusive** and are never second-guessed —
resolution reads two per-user pointers, in this order and each must LOOK like what it
claims to be:

| pointer | shape required | resolves as |
|---|---|---|
| `~/Library/Application Support/RichOS/corpus` | `ceo/` + `companies/` | `--corpus` |
| `~/Library/Application Support/RichOS/loro-root` | `wiki/` + `loro/` | `--root` |
| `~/RichOS/corpus` | `ceo/` + `companies/` | `--corpus` |

They are **two different names rather than one name with a guess about its shape**, because
loro refuses `--corpus` for a corpus that sits inside a git checkout and says so in those
words — resolving the wrong flag would be `exit 2` on every rotation, forever.

**Nothing creates either pointer.** Not the app, not the bundler, not an installer — there
is no installer. It is an operator act, exactly like the engine's own install pointer and a
machine without one boots saying what it looked for:

```
[richos] loro Tier C: no corpus configured — re-primes carry no company memory
[richos] loro Tier C: tried .../RichOS/corpus — not present
[richos] loro Tier C: tried .../RichOS/loro-root — not present
[richos] loro Tier C: tried /Users/alex/RichOS/corpus — not present
```

And nothing here is ever derived from the executable, the working directory or the engine
directory. Inferring the corpus from the checkout the binary sits in is the one inference
`CONTEXT-CONTRACT.md` §1 names as a larger failure than an error.
- `RICHOS_LORO_LANES` — optional, `entity=lane,entity=lane`. Maps an ECS entity area onto a
  loro company partition. **It is a map, never a rule** — name equality is not a mapping,
  an entity with no lane reads the CEO layer and nothing else and a slice carrying another
  company's item is refused whole. It defaults to each registered entity mapped to a lane
  of the same name, which is an ENUMERATION of the registry rather than a pattern: an id
  that is not registered gets no lane however much it looks like one and a lane keyed by
  an unregistered id is refused at boot as the typo it is. Setting this wins outright,
  including setting it empty, which turns lane narrowing off.

  **Every lane is reconciled against the corpus before anything is sent.** A mapping is a
  claim that a partition exists and loro refuses a lane it does not have — `exit 2: no
  such company partition "x" in this corpus` — which would make every re-prime
  `Unavailable`. So at boot the app asks the corpus which partitions it actually has
  (`loro-context.mjs corpus`, ~0.06 s), drops every mapping the corpus cannot satisfy and
  names each drop on stderr. A failed probe leaves the map exactly as configured; it is
  never read as "no partitions", because those are different facts.

  **When the corpus has partitions and an entity is bound to none of them**, its compile
  widens, loro returns every lane and the cross-entity guard refuses the slice whole —
  fail-closed and correct and it reaches the CEO as "loro could not be consulted" every
  turn. Boot says so by name.

  **When the corpus is one repository's own record** (`layout: repo` — no partitions,
  `company: null` on every item), the lane map has nothing to narrow and the guard has
  nothing to refuse and a thread bound to a different entity would silently receive that
  product's record under a `COMPANY MEMORY` heading. The registry names the owner from the
  corpus's own root and every other entity's slice is prefixed with one sentence saying
  whose record it is. The owner's own slices are untouched and an unowned root leaves the
  owner unstated rather than guessed.
- `RICHOS_NODE_BIN` — optional; the `node` used to run the two loro entry points.

See it end to end without launching the app:

```bash
cargo run -p richos-core --example loro_reprime_demo -- "what did we decide about X?"
cargo run -p richos-core --example loro_correction_demo   # provisions its own throwaway corpus
```

### Tier E — the evidence lookup (the CEO's own files and never company memory)

Executes §1/§4 of the 2026-09-17 evidence-retrieval ruling. When the compiled slice's own
`coverage` says memory did not answer — `none` or `adjacent` and **only** those two — the
turn looks in the CEO's Workspace evidence zone and appends one labeled block below the
company-memory block, under `FROM YOUR FILES (evidence — not company memory)`.

The two headings carry different epistemic weight and that difference is the whole point:
`COMPANY MEMORY (loro)` means *the company holds this belief*; `FROM YOUR FILES` means *a
document in your Drive says this and nobody has concluded anything from it.* The block
takes **no characters from the memory lanes' budget**, enters no memory page, promotes
nothing and is retained nowhere.

- `RICHOS_EVIDENCE_BIN` — `tools/richos-service/bin/richos-evidence.mjs`. Unset, the app
  looks for that file **beside** `RICHOS_SERVICE_BIN` and uses it only if it is really
  there. Neither present is the ordinary state of an install with no Workspace source: the
  payload renders NOTHING about files, never a claim that the owner has none.
- The evidence zone is derived from the corpus `LORO_CORPUS` already named, so the lookup
  and the compiler cannot disagree about which corpus this is. **`LORO_ROOT` gets no
  lookup** — the in-repo dogfood layout has no `ceo/evidence/` and pointing one at it
  would ask it to read the product checkout.
- **There is no default zone and the entry point refuses to invent one.** `config.js`
  `corpusRoot()` falls back to `~/RichOS/corpus`, which is right for the interactive CLI
  and wrong for a child process launched with launchd's empty environment; so
  `richos-evidence lookup` exits 2 with the reason on stderr when nothing named a zone and
  reads nothing.
- `rich` audience only in v1. A `worker` or `org` request is a reported refusal on both
  sides of the process boundary.

Everything the block may and may not contain — the 600-character Drive excerpt cap, the
three-item cap, mail as subject/participants/dates with no body, quarantined items excluded
on their stored flag, `scopeAllowed` against `governance.json` — belongs to
`tools/richos-service/lib/workspace/evidence-lookup.js` and is not re-implemented here. The
app runs a process and renders the result verbatim.

```bash
```

Transcript and findings: `docs/verification/2026-09-17-evidence-lookup-wired-end-to-end.md`.

## What is proven vs pending

**Proven (live, 2026-08-24):** the round-trip through the full spine — CEO prompt
persisted crash-safe → re-prime identity injected → real Claude replies **as Rich** →
clean render. The Tauri shell builds into a real arm64 binary. `tests/spine_tests.rs` is documented in the layout above.

**Streaming (2026-08-24):** Rich's reply deltas now stream **live** to the UI via Tauri
events (`rich://turn-started` → `rich://chunk`… → `rich://turn-completed`/`rich://turn-error`),
so the UI renders token-by-token and shows a calm "Rich is working" state. Each delta is
appended to the durable ledger FIRST, then emitted — the ledger stays the source of truth,
crash-safety intact. **The full event contract for the UI is in `richos/app/STREAMING.md`** (the UI
builds against it without reading Rust). Clean output preserved: only assistant text is
ever emitted.

**P1.4 continuity, landed (2026-08-24):** turn-boundary rotation (context-watermark +
explicit triggers), self-authored handoff summaries on clean rotation, mid-turn-crash
recovery/replay (bounded to one attempt, clean-render dedup via superseded turns) and the
identity/action-ledger re-prime that structurally excludes false attribution — all wired
in `richos-core::spine` (`LeaseFactory`, `rotate_lease`, `recover_and_replay`) and proven
both headless (`cargo test -p richos-core`, `tests/terminal_session_tests.rs`) and live
against the real binary (`examples/rotation_roundtrip.rs` — a forced mid-conversation
rotation swaps the backing Claude session and the successor correctly recalls the prior
exchange purely via the re-prime payload). Company name, the assertiveness dial and the
worker-status drill-down are also wired end-to-end (Tauri commands → `richos/app/ui/main.js`).
Full detail + the honest gaps (loro Tier-C compiler still a seam contract, not built; no
attention-seam TRIGGER yet — only the persistence + UI-event seam) are in
the spine-seams + rotation brief, 2026-08-24.

**Machinery routing + retention, landed (2026-08-28):** every non-text agent frame is now
routed into a second event family (`rich://machinery`) and retained in a separate
per-thread, day-sharded journal, on ONE per-turn `seq` shared with the assistant text so
"he said X, then ran Y, then said Z" is reconstructible. Proven headless
(`tests/machinery_tests.rs`, driven by wire frames measured against the real
binary) and live (`examples/machinery_roundtrip.rs` — one real tool-using turn, 24 journal
lines projecting to 9 rows, positions 0..=34 used exactly once across both families; the run
is kept at `docs/verification/machinery-roundtrip-2026-08-28.txt`). The emission set the
routing is built against was measured first, not assumed:
`docs/verification/acp-emission-probe-2026-08-28.md` and re-measured against the native
binary in `docs/verification/native-claude-stream-json-2026-08-31/` when §16 deleted the
adapter. Honest gaps: **retroactivity starts here and nothing earlier is recoverable**;
readable thinking text produces no data on either wire; between-turn updates were unrouted —
**CLOSED 2026-08-30, see below**; and there is no toggle, no renderer and no controls —
that is the rest of Phase 1 and it is deliberately not in this work.

**Between-turn traffic has somewhere to go (2026-08-30):** techy-mode §1.5's gap #1 —
*"the client delivers a frame only if `current_prompt` is `Some`; anything the agent emits
at session start or between turns hits no sink at all"* — is closed. The client keeps a
buffer and §1.5's `last_session_meta` slot (last value wins, so a byte-identical repeat is
suppressed rather than journaled once per turn forever); the spine drains it at every turn
boundary and on every technical-view open, stamping `turn_id: None` — the records attach to
the THREAD, because inventing a turn for them would be a false attribution. They render in
their own section under the conversation, never inside it, since a record with no turn has
no position in the conversation to be drawn at.

The standing order is held by an ORDERING RULE rather than by an argument about which
vendor kinds might give a rotation away: the lane is drained as honest traffic BEFORE an
internal turn starts and as `internal: true` immediately after one finishes, so a re-prime,
a handoff or a successor's residue can never render. Proven against a real child process
(`tests/between_turn_tests.rs`; `tests/between_turn_thread_tests.rs`
end to end through `NativeCognition`) and on the rendered surface (`richos/app/ui/tests/techy.js`
checks 18-21, shots `3-1-08` and `3-1-09`).

Honest gaps that remain: the buffer is bounded at 256 items and an overflow is reported as
a marker record rather than kept; `agent_message_chunk` arriving between turns is retained
as an untyped record because there is no turn to attach text to; and the lane is a RELOAD
path like the rest of techy mode, not a live stream.

**The rotation watermark reads the wire (2026-08-29):** the trigger that decides WHEN a
lease rotates is now the adapter's own `usage_update` `{used, size}`, consumed live off the
machinery stream in `deliver` — not from the journal, whose Tier B is evictable. Until a
lease has reported, the chars÷4 estimate remains as the FALLBACK and `context_source()`
returns `Estimated` vs `Measured` so nothing can print a context number without being able
to say where it came from.

What it replaced was measurably wrong in both directions at once: a 200_000-token window
against `"size": 1000000` on all 50 usage events in the 2026-08-28 capture, fed by a
numerator counting prompt and reply characters only — no tool traffic at all. Live, on the
simplest possible exchange, that numerator read **602 tokens against a measured 31_310**
(`docs/verification/rotation-watermark-2026-08-29/live-run.txt`).

**The cadence change is material and goes both ways**, which is the argument for reading the
wire rather than tuning a constant. Recomputed from the raw capture on every `cargo test`
(`tests/watermark_cadence_tests.rs`): mixed traffic rotates 42 % MORE often (turn 328.2 →
189.5) and the old trigger sat 53.9 turns PAST the hard wall; low-tool traffic rotates 44 %
LESS often (turn 302.9 → 435.7); tool-heavy traffic — an orchestrator Rich's actual shape —
had the old trigger firing 5.86× too late (turn 493.0 against a wall at 84.1).

**Mid-turn is the failure this is aimed at and the honest answer is written down rather
than implied.** A `usage_update` crossing 0.95 of the measured window DURING a turn does not
rotate: rotation mid-turn is structurally forbidden (continuity §3.1) and a mid-turn
rotation is worse than a late one. It records the crossing as an `Internal` action against
that turn and forces rotation at the next boundary under `context-critical`, ahead of any
configured ratio. **It cannot prevent a turn already in flight from hitting the wall** — if
that happens the existing crash path (positive termination signal → `interrupted` turn →
`recover_and_replay`) is what catches it and that limit is stated in
`spine.rs::settle_context_pressure` rather than papered over. Honest gaps: the cadence
extrapolation is linear over FRESH single-turn probe sessions, so its turn counts are
ceilings not predictions (real per-turn consumption grows as tool output accumulates) and
nothing here surfaces context state to the UI — `context_source()` has no reader outside
tests yet.

**Voice mode (2026-08-24):** the `◉` toggle is real. Mic -> VAD -> local whisper.cpp
(`small.en`) -> the SAME `Spine::submit_prompt` typed text uses (`Source::Jam`, one thread,
one ledger) -> Rich's streamed reply -> sentence-pipelined macOS `say` -> one continuous,
interruptible cpal stream. **Proven live end to end on this Mac** — real whisper, real Claude,
real speakers. (An earlier note here said the host had no input device at all; an Elgato Wave:3
has since been connected and is the default input, so the device capture path is exercisable.)
**No live partial transcript** (whisper-cli has no partial-hypothesis stream) — a visible
deviation from the UX direction §4.1 sketch. Full measurements, gaps and packaging
requirements: the voice-pipeline brief, 2026-08-24.

**Echo cancellation (2026-08-29):** `crates/richos-voice/src/aec.rs` is a real 2048-tap
partitioned-block frequency-domain adaptive filter, written here rather than vendored (nothing
to name in the open-source license audit). It is bit-transparent while Rich is silent — with a
zero reference the estimate is exactly zero, so dictation and call transcription are provably
untouched — and it costs 0.355 % of one core.

Barge-in has two rules and **which one is in force depends on whether a canceller exists at
all — not on whether it is confident.** With a canceller present: 15 frames within a sliding
25-frame (0.400 s) window, where a frame counts only if the VAD says speech AND the canceller
says near-end. With no canceller: the 5.008 s consecutive debounce, unchanged.

**That gating changed on 2026-09-18 and it changed because the CEO tested it.** He said one
ordinary sentence over Rich on his own rig and Rich did not stop — *"no matter what I said, he
couldn't hear me."* The short rule used to require `confident()`, which on this hardware is
unreachable: measured by `examples/aec_probe.rs` on the built-in speakers into the Elgato, the
echo is real and dominant (24.9 dB over the room noise) and its timing is known to the sample,
but its ENVELOPE correlates at 0.973 while its WAVEFORM correlates at only 0.174 — so
magnitude-squared coherence caps ANY linear canceller at **5.5 dB** here, WebRTC AEC3 and
Apple's VoiceProcessingIO included. Clock drift, loudspeaker overdrive and reverb longer than
the filter were each tested and eliminated. So his rig fell to the 5.008 s continuity race and
nobody speaks for five seconds without a gap: his longest unbroken run was 66 frames = 1.056 s.

**The rule did not need changing. The signal did.** `confident()` asks *is leftover echo too
quiet to reach the VAD's threshold* — the right question before believing a bare VAD verdict.
The conjunction asks *is there more here than the canceller predicts for the reference level it
can see*, which needs no converged filter, because `leak_gain` is a minimum statistic and
`predicted_echo` is therefore deliberately a low estimate. Measured over fourteen echo-only
conditions — two recordings of the CEO's own rig, each across six settings of the volume knob
spanning his volume 60 to 4 dB above Ray's 85:

```text
                                  worst longest run   worst 25-frame density
  VAD speech alone                    23 frames            23 of 25
  VAD speech AND near-end              6 frames             6 of 25
  the CEO's own voice, same rig        66 frames            25 of 25  (saturated)
```

15 of 25 on the conjunction clears the worst echo ever measured here by 2.50x; the same window
on a bare VAD verdict would fire on Rich at 23 of 25. Pinned both ways in
`tests/barge_in_on_the_ceos_rig.rs` — zero barge-ins and zero admitted utterances across all
fourteen echo-only conditions with a positive control that does fire and a barge-in 0.176 s
into the first thing the CEO actually said, with his words kept. The half-duplex taint rule
still rests on `confident()` and so still stays in force on this desk; "headphones recommended"
is still the honest note. Five reproducible rigs carry the evidence: `aec_rig` (offline),
`aec_live`, `aec_probe`, `aec_transcribe`, `bargein_score` (offline and the one that chose the
rule).

**The recording of the CEO's voice is not in this repository and there is no copy of it here to
find.** `richos` is public, so the microphone track named `ceo-rig-2026-09-18-nearend2-mic.wav` is
committed only to the private `richos-hq` repository, under its `fixtures/echo-path/` directory; it
is resolved at run time through `$RICHOS_PRIVATE_FIXTURES` or a side-by-side `richos-hq` checkout.
The rest of that fixture family — the reference track and both sidecars — is in
`crates/richos-voice/tests/fixtures/echo-path/` and none of it carries his voice. Without the
private track the one test that needs it reports `ignored` with a reason naming it — never `ok` —
and the false-positive test above still runs everywhere, because its recordings contain no near-end
voice. What moved and the measurements that say the rest is safe to keep:
`crates/richos-voice/TESTING.md` and
`docs/verification/2026-09-18-the-ceos-voice-moves-to-the-private-repository.md`.

**Foundation only / later legs:** the attention-seam TRIGGER (timers/log-watchers that decide
WHEN to raise a proactive message — `Spine::raise_proactive` is the seam, judgment is not),
a magnitude-domain echo DETECTOR for the barge-in decision path (the linear canceller has
landed; this is what would make barge-in work on hardware whose echo path is not linear —
prototyped and measured at 0.7 % false positives / 73.6 % detection on a real recording, but
not shipped: see `aec.rs`), a live partial transcript, a warm whisper daemon, a Windows
`SpeechSynth` and the rest of packaging — the bundled whisper binary and models (see the
voice brief's size table), which the `.app` still assumes on the host. Signing,
notarization, stapling and the update channel have left this list: the entrypoint produces
a Developer ID signed, notarized and stapled bundle and three public releases have gone
out through it — "Packaging" above. See the feasibility notes in the handoffs.

**Delegated workers, live (2026-08-29):** the engine's worker-lifecycle stream now reaches
the CEO *during* the turn. `rich://worker-upserted` — §13's eleventh event, deferred while
no lifecycle signal existed — carries the same `worker_activity` row a reload projects,
built by the same two functions (`timeline::worker_activity`, the join with its session
clause and `timeline::worker_activity_item`, the row), so the live path and the snapshot
path cannot drift. Before this the join ran only inside `get_timeline`: a delegation
appeared after a snapshot read and showed as a nameless *"Worked"* row during the turn.
The §26 fixture measured it — 0 chips live, 3 after the snapshot — and now measures the
opposite plus the agreement between the two paths.

Three honest limits, unchanged by this work and worth restating rather than rediscovering:
`waiting`, `interrupted` and `failed` have **no witness anywhere** in the hook set, so
`run_ended` crosses the wire as `WorkerState::Unknown` and renders as *Ended · outcome not
recorded* — never as a completion. There is **no poll and no timer**: the event is emitted
when machinery arrives and once more at the turn's end, so between two tool calls a chip
can be up to one tool call stale (bounded, one-directional, never a claim about a worker
that was never witnessed). And the join is **session-scoped**, because `agent_id` is not
globally unique — the clause that keeps another session's worker name and authored summary
out of this entity's thread.
# Retired orchestration experiment

The managed assignment controller and native Claude owned-outcome integration were
removed on September 9, 2026. Ordinary Rich conversation, onboarding, voice,
updates and the pre-existing engine remain. The review and evidence documents
are historical records, not descriptions of installed functionality.
See [rollback record](../../docs/verification/owned-outcome/ROLLBACK-2026-09-09.md).

`crates/richos-user-update/tests/startup_exec.rs` covers the update startup executor.

`crates/richdoctor-core/tests/session.rs` covers RichDoctor's restricted Claude Code session
against a stand-in CLI: the inventory check at the first message, failure naming and that
stopping a session ends its whole process group. `crates/richdoctor-core/tests/profile_fixtures.rs`
checks the shared screenshot format profile against the golden fixtures RichDoctor and the
reporting relay both read (`richos/mobile/service/report/fixtures/`).

The delivered engine is exercised by `tests/delivered_ecs_tests.rs`,
`tests/delivered_loro_tests.rs` and `tests/engine_profile.rs`. Runtime delivery,
owned processes, provider authentication and repository connections are covered
by `tests/runtime_delivery.rs`, `tests/owned_process.rs`, `tests/provider_auth.rs`
and `tests/repository_connection.rs`. `tests/land_checks_user_tools.rs` runs an app
land's merge check in a lease's environment and proves it finds the user's own tools.

`crates/richos-devtools/tests/contracts.rs` compares offline developer reports
with saved outputs from the original tools. This standalone crate also generates
the pinned dictionary and checks UI test timeout arguments. The existing Python
and Node entry points build it with Cargo in the development environment. It is
independent of the app runtime and installed Claude Code hooks. See its
[developer tool notes](crates/richos-devtools/README.md).

## Shared voice infrastructure

The live pipeline stays in `crates/richos-voice`. Its shared model metadata and
JavaScript provisioning live in [the voice component](../engine/voice/README.md).
App source builds require `../engine/voice/models/`: the Rust crate embeds those
files at compile time. An engine update alone does not replace an app's embedded
metadata. Run `bash scripts/voice-component.test.sh` for consumer and relocation
checks, with Node and Cargo on PATH.

### Shared live Mac resources

`mcp__richos_status__system_resources` gives app-managed agents current host CPU,
RAM, memory pressure, swap and up to sixty recent samples. One native sampler runs
once per second while the desktop app is open. Tool calls immediately read its
in-memory cache through an owner-only local socket. No model is called or sent a
message for each sample and existing resource admission controls are unchanged.

Both Rich and Stu receive the read tool. Generated restricted teammate tool lists
include this single read; saved definitions are untouched. Isolated Claude and
Codex review providers receive an MCP endpoint exposing only this read. Fixed-function
tool-free classifiers and titlers retain their existing contracts.

The version 1 response contains `instance_id`, `status`, `age_ms`,
`observed_monotonic_ms`, `current`, `history` and an optional `detail`. Status is
`warming_up`, `available`, `stale` or `unavailable`. Only `available` has a current
reading. A sample older than three seconds is stale. A clock that includes sleep
measures age; comparison with the awake clock discards sleep intervals. A stopped
or crashed sampler returns unavailable. Unsupported platforms do not report fake
zeroes. There is no per-second disk log or process enumeration.

CPU user (including nice), system and idle are shares of all cores together on a
0–100 scale. Used memory is anonymous pages less purgeable pages, plus wired pages
and the physical compressor footprint; file cache is excluded. The native module's
unit test pins that formula on fixed counters. This definition does not promise
exact parity with Activity Monitor's accounting. Pressure is the kernel's level:
`normal`, `warning`, `critical` or `unknown`.

The desktop `system_resources` command reads the same cache. For a diagnostic read
without opening a window, run the app executable with `--system-resources <absolute
engine-state directory>`. `--resources-mcp` serves only the read tool over stdio.
The bounded `system_resources_probe` core example exercises this same sampler and
reports measured CPU, memory and cached-read cost.
