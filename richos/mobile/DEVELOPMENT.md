# Mobile development loop

The headless CLI runs the shared `core/app.js` action handlers. That core imports the reference client's `richos/web/web-app/lib/queue.js` directly; there is no second queue implementation or source fork.

## Headless first

Requires Node.js with the built-in test runner. No npm dependencies are needed. Run from the repository root:

```sh
node --test richos/mobile/test/*.test.js
node richos/mobile/cli/mobile.mjs headless scenario offline-reconnect
node richos/mobile/cli/mobile.mjs headless scenario interrupted
node richos/mobile/cli/mobile.mjs headless scenario revoked
```

`offline-reconnect` queues a message in the selected conversation while offline, reconstructs the application from durable state, reconnects to a simulated Mac that accepts the message but loses its acknowledgement, reconstructs again and advances a controlled clock through the retry deadline. It verifies one logical delivery and the same message ID on retry. No sleeps or real network are involved.

CLI output is JSON with an `ok` flag, elapsed milliseconds and semantic state. Errors exit nonzero with a structured explanation. Each scenario resets its own state. Stateful commands share a persisted headless session in the external cache:

```sh
node richos/mobile/cli/mobile.mjs headless fixture offline
node richos/mobile/cli/mobile.mjs headless action '{"type":"select-thread","threadId":"planning"}'
node richos/mobile/cli/mobile.mjs headless action '{"type":"compose","text":"Hello Rich"}'
node richos/mobile/cli/mobile.mjs headless action '{"type":"send"}'
node richos/mobile/cli/mobile.mjs headless state
node richos/mobile/cli/mobile.mjs headless action '{"type":"network","online":true}'
```

Other commands: `reset`, `restart`, `transport accept|unreachable|lose-ack|revoked` and `advance <milliseconds>`. Fixtures: `offline`, `online`, `queued`, `interrupted` and `revoked`. `action` accepts `compose`, `send`, `select-thread`, `network`, `sync`, `retry` and `discard`. Unknown actions and fixture names fail explicitly.

## The phone web page is retired

The phone web page (a PWA) the Mac used to serve was retired on 2026-10-30; the native RichConnect apps replaced it. Its `richos/web/web-app/lib` modules stay as the reference client the conformance corpus is generated from. The CLI's `pwa` browser target went with the page.

## Native client

```sh
node richos/mobile/cli/mobile.mjs client scenario connection-restart
node richos/mobile/cli/mobile.mjs client scenario recording-interruption
node richos/mobile/cli/mobile.mjs client scenario voice-gestures
```

The client scenarios use replaceable transport/recording ports and execute the actual
client actions. They cannot prove microphone permission or hardware behavior.

`lab serve` starts an expiring loopback protocol server with its own
identity, pairing and synthetic replies; it never opens the user's real Mac identity or
conversation store. `RICHOS_LAB_PORT` optionally fixes its loopback port. For a phone,
expose that loopback port through a private HTTPS test route such as Tailscale Serve.
The lab uses a verified TLS upstream; it does not bypass certificate validation. Its
mode-0600 `lab.json` contains a single-use pairing code and test status. Stop the owned lab
process with SIGTERM and stop its separately started Serve process after testing.

## Registered repository proofs

The repository runner discovers these suites under `richos/app/scripts`:

- `mobile-headless.test.sh`: `npm test` runs the core and CLI tests in disposable external sessions. It verifies persistence across real CLI processes, structured errors, locking, resource selection and the unpaired Release bootstrap. It needs Node and npm, with no browser.

```sh
bash richos/app/scripts/run-tests.sh --no-host-screen --only mobile-headless.test.sh
```

The suite chooses unique scratch on the mounted external SSD and passes a scoped `TMPDIR` to its descendants. An existing external `TMPDIR` is respected; an unset or non-external value uses `/Volumes/E1TB/tmp/codex/`. The caller's environment and nightly configuration are unchanged. Scratch is removed after child processes finish. An absent SSD reports `NOT RUN`, exit 2, consistently.

Each suite has dependency `inputs` and exact-file `covers` rows. Coverage describes the assertions above, not all behavior in a directory. A new unproved module remains `UNCOVERED` even if an input directory selects a suite. On an integration merge, `bash richos/app/scripts/proof-for.sh HEAD^1..HEAD` must exit 0; that command selects proofs but does not execute them.

## Boundaries

- `core/`: real application actions and state, backed by the reference client's queue. Add new behavior here before wiring controls.
- `dev/`: deterministic environment adapters, persistent synthetic receipts and scenarios. The simulated Mac's receipt table models deduplication; it does not test a real Mac server.
- `cli/`: headless persistence, resource selection and the lab and service commands. The same request envelope reaches the development runtime.

Each CLI invocation locks its checkout's target session to prevent overlapping resets or installs. On a crash, inspect `cli.lock/owner.json` and confirm the PID is gone before removing the lock.

## Storage and measurements

Generated resources, command results and screenshots are kept under `/Volumes/E1TB/caches/richos-mobile/<checkout-hash>/`. `RICHOS_MOBILE_CACHE` can select another directory on that volume. The CLI refuses an absent volume and never redirects build output into the repository.

Keep measured timings and private verification reports in `richos-hq`. Measure these separately: a fresh Node process running a focused scenario and direct core execution. Establish budgets from measured results; a green fixture is not evidence about real microphone permissions, radio transitions, APNs or App Store installation.

For new platform behavior, add a narrow adapter and controllable outcomes without importing platform APIs into the core. Update-policy evaluation runs in `core/updates.js` and is exercised through the same client actions in Node. Validate actual store availability and installation separately on real devices.

## Real Mac protocol integration

`bash richos/app/scripts/mobile-mac.test.sh` runs the actual client actions and native-shaped transport adapter against the production Rust phone listener, authentication, intake, Spine and gated timeline. It verifies fingerprint confirmation, signed messages, invalid-signature refusal, deduplication and a fresh reply after client restart. The provider produces deterministic replies and a test adapter replaces the desktop AppHandle bridge. This proves the transport/core integration without using an AI account or a person's conversations.

For a physical test, start `node richos/mobile/cli/mobile.mjs lab mac` with `RICHOS_MOBILE_MAC_ORIGIN` set to your separate HTTPS test origin. Wait for its ready result before reading `mac.json` for the loopback proxy port. Expose that port using a separately owned Tailscale Serve endpoint. The proxy verifies the Rust server's private test CA; the phone verifies the public endpoint with system TLS. No trust bypass is required.

Stop the foreground lab and its separately owned Tailscale Serve endpoint after testing. The lab closes its sockets, retains synthetic timeline/intake/ledger evidence in the cache and deletes its scratch data. `lab serve` remains the faster JavaScript protocol fixture; it is not the production Rust server.


## Part 2 actions and persistence

`client scenario update-controls` drives banner, dialog, blocking and withdrawn policies through the actual client.  The fixture client identifier is deliberately synthetic and is never bundled in a physical-device or Release build.

## Voice submission

`voice-release` submits a held recording immediately; `voice-send` submits a locked recording. `record-send` is the recovery action for an interrupted saved recording. All use the shared outbox. The destination conversation and Mac are captured when recording begins. Cancel never sends and the transcription cannot replace a newer draft. The native adapter hashes and uploads the protected file directly; audio bytes do not cross the JavaScript bridge. A queued file cannot be deleted until its queue entry is resolved. Acknowledged audio remains playable in its conversation while retained; bounded housekeeping never removes unsent or queued files.

The authenticated Mac route accepts at most 60 MB of mono 16 kHz PCM16 WAV and validates an actual duration of at most 30 minutes. Its bounded transcription budget is 90 seconds; the capture ceiling does not guarantee a slow Mac can transcribe the longest possible message within that budget. Failed work retains the recording. It uses the existing local speech stack. Delivery receipts bind the device, client ID, conversation and original audio bytes. Failed transcription does not reserve a message; a lost acknowledgement reuses the durable receipt without transcribing or submitting again. Uncertain intake remains blocked for attention.

`reply-play` synthesizes a completed CEO-visible reply on the Mac and plays the bounded WAV through the native audio session. Stop prevents a late download from starting playback. Audio from a previous conversation cannot begin after navigation. Read the conversation when playback is unavailable or a reply exceeds the playback length limit. Capability negotiation keeps recordings available on older Macs without claiming they can accept voice.


## Native notification and voice proofs

`client scenario voice-restart` records through substituted platform ports, queues a file reference, reconstructs the actual client and sends the same message/file after reconnecting. It proves durable application behavior without claiming a microphone or speech model.

The real Rust lab can enable the installed local speech model with `RICHOS_MOBILE_VOICE_TEST=1`. Set `RICHOS_MOBILE_VOICE_FIXTURE` to a mono 16 kHz PCM16 WAV on the external SSD and `RICHOS_MOBILE_VOICE_PHRASE` to a phrase it contains when running `node --test richos/mobile/test/mac-server.test.mjs`. The test verifies authenticated upload, local transcription, one projected message and generated WAV reply audio. Model startup gets a bounded two-minute allowance; ordinary text proofs do not resolve a speech model. The optional speech check fails if the model is unavailable. It never records the Mac microphone or plays its speakers.

See [native notification operations](service/notifications.md) for APNs provisioning, token rotation, revocation and delivery limits. Notifications are opt-in. The Mac retains registration intent and pending opaque events for retry; the phone retains its preference and registration fingerprint, not an APNs token in shared session JSON. Phone permission denial leaves foreground conversations usable. Closed-app alerts require Apple delivery and a reachable Mac/provider; they are not an emergency-update transport guarantee.


Routine native reconnects are presentation-silent for three seconds: the transport
is offline and the outbox retains work, while the ordinary header shows no connection status at all. Healthy operation is never announced. A sustained interruption shows a calm reconnecting notice. Known
phone-offline, service-unavailable, revocation and incompatibility states remain
explicit. An opening stream is never evidence that the Mac is asleep. Native stream
startup refreshes the public authentication challenge before signing, including after
suspension. Confirmed stream acceptance clears transient errors even without a hello
frame. Headless fake-clock tests protect these transitions.

Native resume requests one previously observed row as well as newer frames. The shared
thread model deduplicates the replay, while that immediate body avoids waiting for the
15-second heartbeat when an existing Mac/proxy would otherwise return an empty tail.
Incomplete reply replay keeps its earlier cursor, so missed deltas remain recoverable.

For user-driven testing, use `lab mac --manual` with a fresh isolated cache. Manual sessions have no automatic shutdown deadline and retain backend state in `manual-data` under that cache. Stop the owned CLI with SIGTERM when testing is finished. Automated `lab mac` runs retain their bounded lifetime. Never leave a user testing against an expiring fixture.

A file Rich attached (round 36 screen 3): start `lab mac` with `RICHOS_MOBILE_LAB_ATTACH` set to one or more absolute file paths joined by `:` and the lab attaches each, as Rich's `attach_file` tool does, under every reply it writes. The phone gets each card at once and a tap fetches the bytes through the production file route (`GET /api/messages?kind=file`) from the lab's own record in its data directory. Use fixture files, never a person's documents.
