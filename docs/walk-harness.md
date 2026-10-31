# Reusable VM walk harness

The harness separates operation timing from scenario coverage. A fast run with a
missing prerequisite is not a passing walk. The offline suites exercise the command
boundaries. Acceptance also requires a live guest: successful webview and Safari
focus/type, key reuse after idle/relaunch, real signed update/rollback and the full
scenario. Keep those measurements in a private handoff with the raw evidence.

## Bounded accessibility operations

From `richos/app/scripts/testvm`:

```sh
./ax.sh VM find --title Settings
./ax.sh VM focus --role AXTextArea --in composer
./ax.sh VM type 'fixture text' --role AXTextArea --in composer --replace
./ax.sh VM type 'fixture text' --app Safari --role AXTextArea --replace
./ax.sh VM click --title Close --in dialog
./ax.sh VM click --id route-choice --in dialog --expect value=1 --json
./ax.sh VM find --role AXButton --in window 'RichOS' --nth 1 --json
```

Matching reads only the requested attributes. Returned nodes carry full attributes
and geometry. Traversal is breadth first. `--first` stops at the first match and
`--nth N` at the zero-based Nth match in that traversal order. These flags deliberately
do not establish uniqueness. Without either flag, `find` is exhaustive and actions
require exactly one match. A node or depth cap is an incomplete result and fails.

Scopes select the first matching dialog, sidebar or composer subtree. `--in window`
selects windows by exact title. An absent scope fails. A composer is its form group,
with a text area's parent as a fallback. `focus` verifies focus before returning;
`type` verifies focus before sending and briefly polls for WebKit's asynchronous
value update without resending text. The text may
contain newlines; no Send/Return is issued automatically. A coordinate click uses
AppleScript and requires the intended process to be frontmost.

`--id` matches the exact `AXDOMIdentifier`; confirm exposure on the target app.
When an explicit role excludes a title/ID match, failure includes up to three
`near_matches` from the same bounded traversal. These are hints, never clicked.
A role-less exact selector still requires uniqueness. Do not silently resolve a
collision by adding `--contains --first`.

The guest side is `ax.py`, which reads each attribute with the accessibility API
(`AXUIElementCopyAttributeValue`) on the element itself, so two controls with the
same label are always two elements. Focusing can rebuild WebKit's AX tree and
invalidate an element. If the subsequent focus read fails, the tool may refresh
the original DOM ID within the same scope, requiring a complete traversal and
one match. It never resends focus; a missing ID or uncertain refresh refuses text.

`click --expect value=1` resolves, presses and observes the postcondition in one
guest invocation. Repeated `--expect` options may check `value`, `enabled`,
`selected`, `current`, `title` or `description`. A state already satisfied returns
`already_satisfied` without pressing again. Otherwise the tool presses once and
observes for up to one second within the existing total deadline. An unobserved
postcondition or an exception after dispatch returns `effect_unknown`; it never
retries the press. Bare `click` remains a dispatch receipt, not outcome proof.
These local attributes do not prove a backend transaction completed: scenario
acceptance must also check its resulting route or other domain state.

All input still targets the owned test guest. No host UI path is added.
The delta scenario uses exact names by default and offers `--tailnet-route-choice`
only for a fixture with the managed route selector visible. It selects the route
by ID and verifies its local value before pairing. Pin Connect's initial managed
and enabled state when exercising that option.

The default 20-second deadline includes host preflight, SSH, guest execution and
rendering. The guest gets a shorter allowance for diagnostics and cleanup. Timeouts
return 124 with a structured classification before partial output. Phases distinguish
`preflight_deadline`, `transport_deadline`, `guest_deadline` and `render_deadline`.
A guest-start marker establishes whether remote execution began; missing transport
evidence never licenses a claim that the app hung. A deadline conservatively leaves
the effect unknown. Stderr includes `ax_timing` with host phase durations and guest
execution duration where measured. `transport_overhead_seconds` is the residual
including remote startup and shell overhead, not a pure SSH measurement. These
phase records are ephemeral under the configured temporary root and cleaned by
the supervisor. Partial output is preserved; the owned command group is killed.
`SecurityAgent` windows and a modal hiding the requested control produce `blocked`,
distinct from `notfound`. An element whose app does not answer an attribute read
(`kAXErrorCannotComplete`, after the AX messaging timeout) ends the read
with `unresponsive`, never a tree with that branch missing; after a press it is
`effect_unknown`. Process errors
retain their exit status. `TESTVM_AX_TIMEOUT` can select a different total bound for
a controlled comparison, but increasing it is not a recovery strategy.

A read still running after 10 seconds (`TESTVM_AX_SLOW_SECONDS`; a `find` answers in
0.01 to 0.5 s) is photographed while it is overdue, once: `sample` of the app under test,
the read's own process (`ax.py`, the AX client), System Events (keystrokes and raw
scripts) and the WebContent process (each main thread's heaviest path), the guest's
second `top` sample and its memory counters, printed after an `ax_slow_read` line on
stderr. The host adds an `ax_host_reading` line (its admission sample and the VM
processes' CPU). The read is never retried and its deadline never moves; the snapshot
only says whether the app was blocked or the guest was starved
(esc-20261021T122950Z-b8b7362a).

## Fixtures and exclusive execution

Use synthetic fixture homes. `fixture.py --source HOME --out NEW_HOME --kind delta`
keeps two bound thread definitions named `Scenario A` and `Scenario B`, removes old
turns, drafts and runtime journals and retains entity/configuration data. The
`long-history` kind preserves the conversation history for a separate AX benchmark.
Both exclude copied keys, Claude credentials, installed update state and retired
engine copies. The pinned engine is supplied separately to `run.sh`. Inputs stay
unchanged; unexpected symbolic links are refused. The output `fixture.json` records
the source and resulting ledger hashes. The caller owns deletion of these copies.

`run-walk.py` holds one of the test VM's two guest slots (`slots.py`:
`<TESTVM_ROOT>/guest.lock` and `guest-2.lock`, default root `~/.richos-testvm`) for exactly
its own run, boot to cleanup and measured CPU use must be below 80%. The slot is released
the instant the run ends; nothing holds a guest between runs (the CEO, 2026-09-27). Since
CEO ruling §77 (2026-09-22) a walk is a test run: it does not take the nightly
`release.lock`, which stays with the nightly/release commands. `--state-dir` is still
accepted and no longer read. Acquisition is nonblocking unless `--wait SECONDS` is given:
then the walk waits for a free slot and retries the CPU admission for at most that long in
total, releasing the slot while it waits on the CPU. Do not wrap a walk in `reserve.py --wait` instead: the walk samples again the
moment the outer command admits it and on a busy Mac that second single sample refuses
the run. With
a slot it refuses a third running guest, creates a unique headless guest and
keeps the slot until `stop.sh` has stopped the captured processes and deleted
the clone. It also cleans up after boot failure or interruption. Only `caffeinate -is`
is used by the underlying VM launcher.

Example, with paths and pinned versions supplied by the operator:

```sh
cd richos/app/scripts/testvm
./run-walk.py --tailnet --bundle "$CURRENT_ZIP" --home "$DELTA_HOME" --engine "$ENGINE_TAR" \
  --previous-bundle "$PREVIOUS_ZIP" --report "$RESULTS/run.json" -- \
  ./delta-walk.py --out "$RESULTS/delta" \
  --previous "$PREVIOUS_VERSION" --current "$CURRENT_VERSION" \
  --endpoint "$PINNED_CURRENT_MANIFEST" \
  --band-box "$X0" "$Y0" "$X1" "$Y1" --band-color "$BORDER_COLOR" \
  --chip-box "$X" "$Y" "$WIDTH" "$HEIGHT"
```

The runner inserts its VM name immediately after the scenario executable. The
previous bundle is staged into that guest and passed as `--previous-app`. The
manifest URL must include `/releases/download/v<CURRENT_VERSION>/`. Version
arguments themselves omit the `v` prefix. The previous app's bundle version is
checked before it is installed in the disposable guest's user Applications folder.
Only the real updater creates receipts. `rollback.py --check-only` validates an
existing history before any UI exploration; missing history is a prerequisite
failure. The scenario takes the signed update, activates it, reads the updater's
receipts, rolls back, then checks both boot identities and the new update offer.

Place results, fixture copies and the configured `TMPDIR` on the designated scratch
volume. Keep result files outside disposable fixture directories. Reports and OCR
caches can contain fixture text and paths; do not publish them without the privacy
gate. After inspection, delete the caller-owned scratch and caches.

## Scenario evidence and limits

`scenarios/delta.json` specifies order, dependencies, limits and outcomes. The phone is
the actual RichConnect client run on the host by `walkphone.py`
(`walk-phone.mjs` beside it), over the native apps' API and the guest's tailnet
name, so the walk needs `run-walk.py --tailnet`: it pairs and it answers the six words
for the rejection check. The steps that photographed the retired phone web page in the
guest's Safari (typed-to-phone, reply-to-phone, phone-to-Mac) are gone with that page
(2026-10-30): a native phone has no screen in the guest. Five Mac turns supply the
first-word samples. One more Mac turn supplies the thread-switch capture; it requests a
12-second terminal wait before its reply so the eight-second switch has an active-work
interval to observe. Send attempts count before sending;
there is no automatic retry and recovery cannot exceed six total attempts. A missing
reply is a measured failure, not a reason to keep sending diagnostic prompts.
On a shared host, `--admission-wait-seconds 120` allows a bounded wait for user CPU
below 80% before each attempt. Each probe reads the kernel's own counters in process
(`host_statistics` and `sysctl`, no `top`, `lsof` or other subprocess) twice, one second
apart. User CPU excludes system time: under memory pressure the compressor and pager
inflate system time and that condition is judged by the memory rule instead. The
memory rule refuses while kernel pressure is CRITICAL or swap-out during the probe is
at least 16 MB/s; pressure, free percentage and swap in use are reported beside it.
Probes are at least 30 seconds apart, so a waiting caller never spins and the final
probe can extend the requested wait by about one second. Actual admission time, the
probe count, measured CPU and memory figures and informational load average are
recorded in each capture receipt. Admission time is included in the complete scenario
duration. Busy CPU, hard swapping or an unavailable measurement refuses admission
before consuming a turn or sending a prompt. Load average alone
neither establishes another active job nor blocks work on an otherwise idle CPU.
For an explicitly controlled benchmark, `reserve.py --max-load N` retains an opt-in
legacy load criterion. `reserve.py` alone takes no lock (§77); `run-walk.py`'s guest
lock remains mandatory for a walk in either mode and `reserve.py --release-lock` holds
the nightly lock only for work that writes nightly or release state.

Each prompt contains spaced characters; the requested reply joins them. Thus the
reply token is absent from the prompt. Offsets use the capture's millisecond
timestamps, including its action window. Keep the frame cadence and action window
alongside the result when interpreting subsecond numbers.

Captures use a fixed desktop rectangle. Check the supplied band boundary and chip
rectangles against a guest frame before acceptance. Band and chip coordinates are
relative to the desktop crop. The band check examines every
band-visible frame and requires an exact boundary color measurement, without a
tolerance that could also match the surrounding background. The old detail
line is searched across the complete frame stream. A switch at eight seconds only
qualifies if work is visible immediately before it; otherwise the chip check reports
`prerequisite unavailable`. AX must also confirm that the destination is selected.
These checks fail closed if a changed layout no longer exposes the expected evidence.

The rejection check runs before rollback. It verifies the stored rejection survives
relaunch, the rejection explanation is visible and the app has no listener on 8443.
An expired pairing URL alone is insufficient evidence.

Every step reports `PASS`, `product failure`, `harness failure` or
`prerequisite unavailable`, elapsed time and evidence or a failure reason. Failed
dependencies name the skipped work; independent steps continue. `report.json` gives
scenario coverage and model attempts. The outer `run.json` separately records boot,
scenario, cleanup and complete elapsed time. Use the inner report for classifications;
a nonzero scenario exit alone cannot distinguish a defect from a missing prerequisite.

## Keychain endurance outside routine timing

`keychain.sh prepare` configures and reads back the GUI session's default/search list,
unlock state and no-timeout settings. Its prerequisite probe uses the default search
list with normal access controls. It does not use `-A` or broaden actual app-key ACLs.

Add `--keychain-endurance` to the delta command for a separate run with no test sends:
initial pairing, over 30 minutes idle, relaunch and existing-key reuse. Alternatively,
run `keychain-verify.py VM --output REPORT` against an already paired owned guest
inside the reservation. `--idle-seconds` must exceed 300; the default is 1801.
The app can still make automatic initialization turns at boot or relaunch. Report
those separately from test sends; a zero-send run is not necessarily zero model use.
It fingerprints the app's three actual accounts under the service derived from its
guest data directory. Secrets never leave the guest. A listening channel after
relaunch and unchanged keys provide the reuse evidence. SecurityAgent is sampled
every 15 seconds and at access boundaries; this cannot prove that a shorter transient
prompt never existed. Preserve guest capture/log evidence for that stronger claim.
A remaining prompt or timeout fails the prerequisite and may require a product fix.

## OCR reuse and comparisons

Set `RICHOS_QA_OCR_CACHE` to a private per-walk cache directory. `ocr-find.sh` accepts
repeated `--pattern`, `--timeline`, `--json` and `--flat` flags. With several patterns,
`--first` means the first hit of each pattern and exit 0 requires all patterns.
Exit 1 means an absent pattern; exit 2 means analysis could not run. Omit `--first`
for an absence claim that needs every frame.

Cache identity includes image bytes, reader bytes/version, options, locale and OCR
configuration file contents. Successful empty reads are cached; failures are not.
`ocr-gate.sh` uses cached text but evaluates current privacy rules every time. Its
positive control always invokes the reader again. No cached clean verdict exists.

For an AX speed comparison, use the same long-history fixture hash, guest resources,
product build and starting UI state for baseline and candidate. Time each known
lookup/focus/type/click separately and verify its effect, including the app and
Safari fields. Record absent and modal cases separately. For OCR, run the two queries
on identical immutable frames with identical reader/configuration settings; record
cold and warm times and verify identical hits. Do analysis after timing captures.
Report complete scenario duration with coverage separately from these operation
comparisons. An unfinished historical run is not an equivalent speed benchmark.

## A fresh install and work given from the phone

`adopt-walk.py` needs no source home: `run-walk.py --home` gets an EMPTY directory and the
walk goes through the app's own first run (memory setup declined, a company registered
with a folder that it makes into a repository in the guest, the business questions
declined; the first conversation is the app's own). It then starts `adopt-watch.py` in
the guest, pairs a phone over Tailscale (both sides press `They match`, Sage F1), sends a
task from the phone's composer and passes only when an
assignment on that conversation leaves Registered no earlier than the phone turn's
`TurnCompleted`, with every prompt on the conversation from the phone's intake channel.
The phone is the actual RichConnect client on the host (`walkphone.py`), over the native
apps' API, so the walk runs with `run-walk.py --tailnet`; the guest's Safari drove the
phone web page, retired 2026-10-30.

```sh
cd richos/app/scripts/testvm
./run-walk.py --tailnet --wait 600 --bundle "$ZIP" --home "$EMPTY_DIR" --engine "$ENGINE_TAR" \
  --report "$RESULTS/run.json" -- \
  ./adopt-walk.py --out "$RESULTS/adopt" --expect-sha "$SHA" --within 240
```

`adopt-watch.py` writes every assignment state, prompt and intake record on the guest's
clock and never the words of a prompt. `--steps` runs a subset, still as one run under
`run-walk.py`. `hold-walk.py`, which kept a guest up for hand-driven steps, is retired
(the CEO, 2026-09-27): a step found by hand with `ax.sh` and `shot.sh` goes into a script
run once under `run-walk.py` and the slot is free again while its author thinks. The phone
sheet names the Tailscale account it is signed in with; screenshots of it are not for a
public record.
