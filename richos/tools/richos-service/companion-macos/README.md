# RichOS macOS capture companion (P2)

The native macOS half of the call-transcription architecture
(2026-08-24, §5.2 + the P2 row of §9). It
closes the gap the browser extension structurally cannot reach — **desktop-app calls (Zoom/Teams
native) and in-person conversations** — by capturing OS-level system audio + mic with **no bot** and
**no per-call gesture**, then writing P1's **frozen capture→pipeline contract** so the existing
pipeline (`richos/tools/richos-service/`) transcribes its output unchanged.

The companion is "just another producer" of the same session directory the extension writes. It adds
zero pipeline code; it only produces the contract.

## What it captures (architecture §5.2)

- **System audio** via a **Core Audio process tap**: `CATapDescription(stereoGlobalTapButExcludeProcesses:)`
  → `AudioHardwareCreateProcessTap` added to a private **aggregate device**, read with
  `AudioDeviceCreateIOProcIDWithBlock`. Captures **all** system output (Granola parity, §10-Q2
  "all-output first"). Audio keeps playing to the user — the tap reads a copy of the render stream
  *before* the physical output device, so it works on headphones exactly as on speakers.
- **Microphone** via `AVAudioEngine.inputNode`, resampled to the tap's sample rate so the two async
  sources stay frame-aligned.
- **Mixed to the frozen 2-channel contract: mic → LEFT (me), system tap → RIGHT (others).** Never a
  pre-mix — free "me vs them" separation with no diarization model.

Uses the **narrow "System Audio Recording"** TCC permission, **not** broad Screen Recording.

## The permission gate (honest)

The first `capture` run triggers the macOS **"System Audio Recording"** prompt (plus the standard
**Microphone** prompt). Both are **human GUI grants** — there is no non-interactive path and no API
to query current grant status (architecture §5.2). So:

- **Verified on this Mac WITHOUT any grant:** everything except the live OS tap — the companion
  launches, writes `session.json` at capture START, opens the WAV, writes `health.ndjson`, mixes the
  frozen L/R channels, and produces a valid contract directory that **P1 transcribes into a
  speaker-attributed `transcript.md`** (see "Verification" below). This is proven via the headless
  `ingest` path, which runs the *same* `SessionWriter`/`ChannelMixer`/`WavWriter` code the live
  capture uses.
- **Needs one human grant to exercise:** the live `capture` command's actual Core Audio tap +
  mic. To grant: run `richos-companion capture`, approve **System Settings → Privacy & Security →
  System Audio Recording** (and Microphone) when prompted, then re-run. The code compiles and is
  written to the Core Audio tap API per architecture §5.2; it cannot be end-to-end-verified in a
  non-interactive environment because the grant is a GUI action.

Because `session.json` is written at START (`status:"open"`), a capture that never gets the grant
leaves a **loud open/empty session on disk** — the never-silent anomaly the pipeline's reconcile
guard flags — never a silent absence (§6.1).

## CLI

```
richos-companion doctor
    Report macOS version, tap availability, pipeline deps, and the permission gate.

richos-companion capture [--zone DIR]
    Live capture: system-audio tap + mic → 2-channel contract dir. Needs the TCC grant.
    Ctrl-C finalizes the session (status "closed") → the P1 watcher transcribes it.

richos-companion ingest --stereo <wav> [--zone DIR] [--started-at MS]
richos-companion ingest --mic <monoWav> --system <monoWav> [--zone DIR]
    Headless: push a pre-made sample through the REAL contract writer (no grant) → a valid session
    dir the P1 pipeline transcribes. This is the on-Mac proof of the contract + handoff.

richos-companion watch [--exclude-pid N]... [--exclude-bundle ID]... [--poll S] [--trace]
    Report OTHER processes' microphone use as JSON lines (Core Audio's per-process
    kAudioProcessPropertyIsRunningInput; no permission, no device opened). The app's meeting
    prompts and end-of-call detection read it (Granola PRD V4). RichOS's own voice/dictation and
    the capture process are excluded by the caller. Exits when stdin closes, so it dies with the
    app that started it. --trace adds every process's input/output flags, for measurement only.
```

Drop zone resolves exactly as the pipeline's `config.js#dropZone` does — `$RICHOS_DROP_ZONE`, else
`<$LORO_CORPUS|~/RichOS/corpus>/{companies/$RICHOS_ACTIVE_COMPANY/evidence|ceo/evidence/unfiled}/meetings`
— and **refuses a zone inside this repository**, which ships publicly. The rule and the 2026-08-29
finding that made it necessary are documented on `DropZone` in the core target; `doctor` prints the
resolved path and which rule produced it.

## Build & test

```
swift build
swift test        # deterministic unit tests: channel mapping, mix/failover, contract shape, WAV,
                  # coordination parsing, drop-zone resolution + the in-repo refusal, the
                  # microphone-activity tracker behind `watch`
```

The proof selector's seat for this package is `richos/app/scripts/companion-macos.test.sh`: the
unit tests (it reads the passing count back) and `watch`'s exit-on-stdin-close contract.

```
```

## Reliability model (matches the shared guarantees, §6)

- **Never-silent (§6.1):** `session.json` written at capture START; a call that captured nothing is a
  loud on-disk anomaly, not an absence.
- **In-call health (§6.2):** `health.ndjson`, one row per second — per-channel peak/RMS, byte growth,
  tap/mic liveness, level, problems.
- **Outside watchdog (§6.3):** the companion holds its own 1 Hz timer; **no byte growth for 15 s**
  raises a loud alarm (stderr + macOS notification) — catches a stalled tap, a device change, or a
  **revoked System Audio Recording permission** (which macOS gives no query API for).
- **Mic-vs-tap failover (§6.4):** if one source stalls while the other is live, the writer silence-
  fills the dead side and keeps the live side recording, and alarms — it never freezes the whole
  recording. (`MixDecision`, unit-tested.)
- **Recoverable audio (§6.5):** the WAV is fsync'd and its RIFF/data size headers rewritten every
  second, so a crash loses at most ~1 s and the on-disk file is always a decodable prefix.

## Audio format choice (single-part WAV) — a flagged, architecture-sanctioned decision

The architecture §3.1 sanctions **"Opus-in-Ogg OR 16 kHz WAV."** This companion writes a **single self-contained
2-channel 16-bit PCM WAV** (`audio-part-00.wav`), for two concrete reasons:

1. **The frozen pipeline's multi-part concat only remuxes into a `.webm` container** (`normalize.js`:
   `-f concat ... -c copy _concat.webm`). WebM accepts Opus but **not** PCM, so *multiple* WAV parts
   would break the pipeline (verified: `Only ... Opus audio ... supported for WebM`). A **single**
   WAV part takes the pipeline's `parts.length === 1` direct path and works unchanged.
2. **Core Audio has no native Opus encoder**, so Opus would add a runtime dependency the "minimal
   native footprint" (§1) argues against.

Single-part + continuous-write-with-periodic-fsync is exactly the companion recoverability model
§6.5 describes ("continuous write … worst case one chunk"). Multi-part Opus rolling (for
belt-and-suspenders part isolation) is a clean later refinement if the pipeline's concat step is
extended to a PCM-friendly container; it is **not** required for the contract or the reliability
guarantee, and doing it now would break the frozen pipeline.

## Verification (run on this Mac, 2026-08-24)

- `swift build` + `swift test` → **36/36 pass** (channel L/R mapping, Int16 scaling, downmix,
  mix/failover decisions, contract v2 shape incl. explicit nulls, WAV round-trip, full SessionWriter
  contract-dir write, drop-zone resolution + the in-repo refusal). *Re-run 2026-08-29 on macOS 15.6 /
  Swift 6.1.2. This line previously said 18/18, which had been true when it was written and was not
  re-derived after tests were added — the same defect class as the drop-zone comment below it.*
- `ingest` produced a real contract dir from a two-voice `say` sample (mic line + system line as
  separate mono WAVs), then **P1's pipeline (`node bin/richos-service.js run`) transcribed it to a
  speaker-attributed `transcript.md`** — `Me:` = the LEFT/mic line, `Them:` = the RIGHT/system line,
  proving the frozen channel mapping end-to-end. `pipeline.status="ready"`, `modelRuns` recorded,
  `_ingest.jsonl` line written with `source:"desktop-companion-macos"`.
- **The P1 drop-zone watcher** (`scanZone`, process mode) **auto-detected the companion's closed
  session and transcribed it** — the real automatic handoff, unchanged from P1.
- **Pending the one human grant:** the live OS-level tap + mic capture (see "The permission gate").
  The step-by-step for closing it — including how to tell a grant that was *given* from one that was
  merely *prompted for* — is [`MACOS-TEST-PROTOCOL.md`](MACOS-TEST-PROTOCOL.md), the peer of the
  Windows companion's `WINDOWS-TEST-PROTOCOL.md`. **Nothing in this README claims the live tap
  works**; §6 of that protocol lists what a completed run still would not cover.

## Not in scope here (later phases)

Extension↔companion session-ownership coordination / double-capture suppression / failover promotion,
and the real loro-correction corrector, are **P4** (architecture §9). This is P2: OS capture + the frozen
contract + the reliability model + the proven P1 handoff.
