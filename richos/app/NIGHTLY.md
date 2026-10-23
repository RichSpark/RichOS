# Manual local nightly releases

A nightly is released **only when someone explicitly runs the release command**.
There is no scheduler, watcher, push hook or GitHub Actions release workflow.
The Mac must be awake, online and able to access its signing Keychain.

## Commands

From the repository root, check readiness without building or publishing:

```sh
python3 richos/app/scripts/nightly-local.py check
```

To build a nightly with the full release gates and have the lead finish publication
after QA and changelog review:

```sh
python3 richos/app/scripts/nightly-local.py release --gates-at-once all --simulated-phones 2
```

`build`, `release` and `stable` refuse to start unless `--gates-at-once` is on the
command line, because nobody should wait an hour on a free Mac for a default nobody
chose (CEO, 2026-09-25). The run log's first lines record the values and who chose them.

- `--gates-at-once N|all` -- how many gates run at the same time. `1` is the old
  order, one after another; `all` starts every gate whose inputs are ready (the lint
  waits for the script suites' receipt, the privacy sweep for the UI suite to put
  the tree back). One failing gate refuses the build and stops the others.
- `--simulated-phones N` -- optional: how many simulated iPhones the suites may use at
  once, one per device type. No suite in the desktop build leases one, so it is not
  required; the machine still boots at most two simulators at a time.

Choose from what else is running: `all` on a free Mac, fewer when engineers are busy. The worker-token, simulator and CPU/memory admission checks
still refuse what the Mac cannot carry.

The command builds the current remote `main`, not uncommitted work or the current
local branch. It uses a dedicated detached worktree at `~/.richos-nightly/source`.
It never switches or resets the developer's checkout. If the dedicated worktree
has unexpected changes, it refuses rather than deleting them.

If that source commit already has a successful nightly, it does nothing. To
explicitly rebuild the same source with a fresh version:

```sh
python3 richos/app/scripts/nightly-local.py release --force --gates-at-once all --simulated-phones 2
```

The command runs in the foreground. Logs and release artifacts are under
`~/.richos-nightly/logs` and `~/.richos-nightly/releases`. One local command can run
at a time. A failure leaves the previous published nightly available. Rich owns
recovery and publication from the recorded candidate run. A failed upload is retried
with `publish --run`, preserving the tested build and its approved notes. No release
scheduler is installed.

## QA before publication: `build`, `candidate`, `publish`

CEO ruling, 2026-09-17 (`ceo-decisions.md` §45 addendum 2): QA walks the
nightly's own candidate before it is published; publication happens only on a
READY verdict. Both `build` and `release` stop at the signed candidate and
automatically request its changelog worker. `release` retains its full-gate policy;
`build` can reuse qualified gates. Rich publishes after the accepted candidate’s
notes have been independently reviewed, landed and pushed to main:

```sh
python3 richos/app/scripts/nightly-local.py build --gates-at-once all --simulated-phones 2
```

A `build` retry automatically reconciles completed gates from earlier attempts.
Each actual gate keeps its own immutable log, input identity and status under
`~/.richos-nightly/gate-evidence/<run-id>`. Matching source bytes, dependencies,
external fixtures and execution settings let a completed pass stand while failed,
stopped and unrun gates receive the first available slots. Changed or unreadable
inputs and damaged evidence require execution. Earlier failures remain failures.
The candidate records the original passing runs in `gate_evidence`; `REUSED` does
not claim a new passing invocation. UI reuse runs the existing coverage verifier
on a frozen copy of every receipt. The release smoke runs on every attempt and
`release` retains fresh gates; stable follows its promotion gate policy below. Older runs without these input records
cannot be retroactively qualified from their logs.

Builds the signed, notarized app against a verified engine pin, then stops for QA.
The candidate has a continuous number and a remote `refs/candidates/<number>`
reservation. It creates no public version tag or release-list entry. The engine
is uploaded under its digest on the existing `nightly` channel release and read
back before compilation. These assets are publicly downloadable and visible in
that release's Assets section. The candidate app is local and the update channel
does not move. It prints the number, run ID, staged path and instructions for QA:

```
Candidate build 24: v1.2.0-nightly.20260921.24, source aa0165cc...
  staged at : ~/.richos-nightly/releases/v1.2.0-nightly.20260921.24
  bundle zip: .../RichOS-1.2.0-nightly.20260921.24-macos-aarch64.zip

To walk it: unpack into a scratch HOME so it never touches the operator's own
app data (README.md's activation invariant D), and run it directly rather than
via `open`, so a harness/terminal holds the process (invariant P). Set
RICHOS_ACTIVATION=regular so the window is visible instead of accessory-hidden:

  mkdir -p "$TMPDIR/richos-qa-<run-id>" && ditto -x -k '<bundle zip>' "$TMPDIR/richos-qa-<run-id>"
  HOME="$TMPDIR/richos-qa-<run-id>/home" RICHOS_ACTIVATION=regular \
      "$TMPDIR/richos-qa-<run-id>/RichOS.app/Contents/MacOS/richos-tauri"

Publish only on a READY verdict:  nightly-local.py publish --run <run-id>
```

A walker (or a walk brief) who only needs those instructions again, without
rebuilding, can ask for them directly:

```sh
python3 richos/app/scripts/nightly-local.py candidate --run <run-id>
```

`candidate` reads only the recorded build; it never runs any command and never
touches the network.

### Changelogs are ready before publication

A completed build saves its exact-candidate notes request under
`~/.richos-nightly/changelogs/<build-commit>/request.json`, outside the sealed
candidate. A durable escalation starts Rich’s dedicated release-notes writer while
QA walks the build. The writer follows `scripts/changelog-instructions.md`, writes
on its own branch and hands over. The request supplies the exact net changed-file
inventory and a compact commit index. Newt establishes the before/after behavior,
actors and conditions from the pinned implementation and acceptance evidence before
writing. Its private facts account covers included, combined and excluded changes.
Rejected drafts never land. A replacement still
compares with the last successfully published release of its channel.

After QA acceptance and Newt’s completed source-facts work, Rich lands the corresponding Markdown
entry and its record in `docs/release-notes.json` together. `publish` reads remote
main and requires the matching version, source commit, build commit, baseline and
prose. The same prose appears in the GitHub release body at creation. A usage hold
can delay publication, but cannot leave a public release waiting for its notes.

If the baseline changes before the release body is public, regenerate the request
and have Newt update the source facts and replacement notes. The draft command updates the recognized unpublished
entry and registry together. Once a release body is public, its notes are retained
on upload retries. Each entry carries its own GitHub release link. Rich publishes
as the very next step after landing and keeps the same release job open until the
body and installable channel are verified. The link can briefly precede the release
because these are separate operations. Retry upload failures with the same candidate
and notes. If the candidate is deliberately abandoned after landing, remove its
entry and record through the normal gate before closing that job.

### A build opens nothing on this Mac and proves the boot in the test VM itself

CEO, 2026-09-19: *"So, every engineer will keep opening the app making me
unable to do anything here or WHAT???"* — `gui-boot.test.sh` boots the real app
on the real screen for ~162 s of every build.

**Since 2026-09-29 this is what `build` does with no flag at all.** Once the
signed candidate exists, the build boots that bundle in a test-VM guest
(`gui-proof-in-vm.sh`, below), keeps the proof at
`~/.richos-nightly/gui-proofs/<run-id>.proof` and `publish --run <run-id>`
uses it without `--gui-proof`. A proof that does not pass fails the build after
the candidate is printed and says how to take it again without a rebuild.
`RICHOS_GUI_HOST=<guest>` names the guest and `TESTVM_*` settings reach the
VM step; any other `RICHOS_*` / `RUN_TESTS_*` setting no step would receive
refuses the run before anything starts, instead of being dropped.
`--host-screen` puts the screen suites back on this Mac's display (never on the
CEO's Mac, §65). `gui-boot.test.sh` itself still records NOT RUN (no screen):
the guest has no cargo and no repository to build its debug binary.

```sh
python3 richos/app/scripts/nightly-local.py build --gates-at-once all --simulated-phones 2
```

holds back every suite that boots the shipped binary, so the candidate is
built, signed, notarized and walkable without one pixel reaching this machine.
It is accepted for `build` and refused for `release`, which publishes in one
motion with no `publish` step to refuse an unproven boot.

**The price is real and `publish` collects it.** Such a candidate records
`gui-boot.test.sh: not-run` in its `build-info.json` and `publish` refuses it
until somebody has watched it start. The build takes that proof itself; by hand
(a build whose VM step failed or an older candidate) it is:

```sh
richos/app/scripts/gui-proof-in-vm.sh --run <run-id>
python3 richos/app/scripts/nightly-local.py publish --run <run-id> --gui-proof <the file it names>
```

`gui-proof-in-vm.sh` boots the candidate's own signed bundle inside a `testvm`
guest (`docs/testvm.md`) on an empty fixture home, reads `windows=N` back — a
pid is not a proof; the app can run and draw nothing — writes the proof and
stops the guest. Nothing appears on this Mac's screen.

A proof must name **this** candidate's commit, exactly as `--checks-done-at-land`
must name the commit it fetched; one taken against another tree is refused.
Two kinds are accepted and they are not the same claim:

| `suite=` | what it proves | written by |
|---|---|---|
| `gui-boot.test.sh` | a debug binary built from the checkout, held to B0–B8 and C1–C5 on a synthetic machine — engine resolution, the plist's shape, the company registry | `run-tests.sh --only gui-boot.test.sh --proof-out <path>` |
| `shipped-bundle-boot` | **the artifact this release will publish** — signed, notarized, stapled — started on a clean guest with no developer environment and drew a window. Fewer assertions, truer artifact | `gui-proof-in-vm.sh` |

A candidate built before this field existed publishes as it always did: those
builds ran `gui-boot.test.sh`, because it was not skippable then.

Once QA returns a READY verdict, publish the SAME candidate:

```sh
python3 richos/app/scripts/nightly-local.py publish --run <run-id>
```

`publish` refuses rather than uploading anything if the candidate's recorded
files (the app archive, its updater `.sig`, `latest.json` — version, SHA and
signature are all just files under the staged directory, so one check covers
all three) no longer match what `build` produced or if the built source
commit is no longer an ancestor of the current `main` — a rebase or a
force-push that happened while a walker was on the candidate. Neither `build`
nor `candidate` needs a signing or notarization credential to have reached
this point; only `build` does (it is the step that signs and notarizes) and
`publish` does not re-request one.

`--run <run-id>` is the id `build` printed. It is looked up under
`~/.richos-nightly/runs/<run-id>.json`, which just points at the staged
candidate directory `build` already produced — nothing is copied or rebuilt.

## Stable: a rebuild of a nightly the CEO has tested himself

A stable release is **never** a fresh build of today's `main` and it is **never** a
published nightly's bytes re-tagged. It is a **rebuild of the commit a published
nightly was built from** — copied from T3 Code, whose release workflow says it in one
sentence: *"Manual stable releases build the commit of the latest published nightly,
so stable only ever ships a build that nightly users have already run."*

When he says "promote nightly N", it is ONE command, with his sentence verbatim:

```sh
python3 richos/app/scripts/nightly-local.py stable --from-nightly v1.2.0-nightly.20261018.65 --ceo-words '<his sentence, verbatim>'
```

Nothing is prepared first. His words are his decision for that tag, dated today and
checked by the same rules as a written record entry (below). The gates are not run
again: the run on this Mac that built that nightly is found in `~/.richos-nightly/logs`
by its own `Building <tag>...` line and its gates are reused with every refusal of
`--gates-passed-in` (every gate passed, same source commit, same installed
dependencies). When an attempt failed and its retry built the same tag, the run that
finished (its `phase timings` summary follows the line) is the one. If no single run
says it built the tag, the command says so and stops;
it never reruns the gates on its own. `--gates-passed-in RUN` names the run explicitly;
`--gates-at-once N|all` runs every gate again instead.

The rebuilt bundle is a new artifact (another version, signed again), so it is booted in
the test VM exactly as a nightly candidate is (`gui-proof-in-vm.sh`, never on this Mac's
screen) before anything is uploaded. A boot that is not proven stops the command with the
version unpublished.

```sh
python3 richos/app/scripts/nightly-local.py stable --from-nightly v1.2.0-nightly.20261018.65 --ceo-words '<his sentence>' --dry-run
```

`--dry-run` prints exactly what would be built and published — the source commit, the
version, the endpoint that would be compiled in and his recorded decision — and does
nothing else. No tag is created, nothing is built, uploaded or published.

### Only the CEO promotes a nightly

**The command refuses unless his own decision to promote that exact nightly is
recorded.** His words, 2026-09-20 (`ceo-decisions.md` §69): *"Stable can ONLY EVER be
build from a nightly if I have extensively tested that nightly and deemed it good
enough to be promoted to stable."*

A READY verdict, a gui-boot proof, a green gate and an Urban signoff are preconditions
for **publishing a nightly**. **None of them promotes anything** and none of them is
accepted here. "Extensively tested" is his own use of that nightly, for as long as he
chooses; nobody else's test counts toward it. There is no override flag and no
environment variable — a decision a stray export could make would not be his.

His decision is given with `--ceo-words` on the command line or as an entry Rich
wrote earlier in `richos/app/stable-promotions.json` on main. Either way the stable
release commit carries it: its own `stable-promotions.json` gains the entry and its
`nightly-build.json` and release notes name the nightly, the date and his words. The
record's shape:

```json
[
  {
    "tag": "v1.2.0-nightly.20260920.1",
    "decided_on": "2026-09-20",
    "words": "<his sentence, verbatim>"
  }
]
```

An entry naming a different tag, missing his words or dated malformed is refused and
so are two entries for one nightly, including `--ceo-words` for a nightly main's record
already decides on.

### Why a rebuild and not the bytes

Re-tagging a nightly's bytes as stable would ship a broken stable release and two of
the three reasons are properties of the bytes that re-tagging cannot fix:

- **The update endpoint is compiled into the binary.** A promoted copy would fetch
  from the nightly channel forever, silently — and as this document says elsewhere, an
  endpoint cannot be changed by an update that copy can no longer fetch.
- **The version string would be `1.2.0-nightly.N`** — a prerelease, superseded by the
  real `1.2.0` it was meant to be.

The guarantee worth having is *no stable ships a commit nobody ran*. That is a property
of the **commit**, never of the bytes.

### What it does

1. Reads his promotion decision for that tag. This is first and it costs nothing.
2. Resolves the nightly's **source commit** from that nightly's own tagged provenance.
3. Takes the version from **that commit's** `Cargo.toml` — a stable release of commit
   X ships X's own version, whatever `main` has since bumped to.
4. Moves the dedicated worktree to that commit. Its gates already passed there in the
   run that built the nightly, against the dependencies installed now, so none runs
   again (CEO, 2026-10-19); `--gates-at-once` runs every one of them anyway.
5. Builds, signs and notarizes with the **stable** endpoint compiled in.
6. Records the completed candidate, requests its changelog worker and boots that
   rebuilt bundle in the test VM. A failed boot leaves it unpublished. The notes
   cover everything since the previous stable, including intervening nightlies.
7. After acceptance and independent notes review, Rich lands and pushes the entry
   and approval record, then runs `publish --run <stable-run-id>`. Publication requires
   the rebuilt bundle’s own passing proof at its exact build commit. A nightly or
   debug-build proof cannot substitute. It uploads assets while the release is still
   a prerelease, verifies them and flips `--latest --prerelease=false` only afterward.
8. A separate next-version worker advances the next patch (`1.2.0` to `1.2.1`) in
   `src-tauri/Cargo.toml` and the app’s `Cargo.lock` entry on its own branch. Rich
   reviews and lands it through the ordinary gate. The release command never writes
   main. A main already past the released version is left alone; the existing
   collision guard prevents another nightly from using a published stable version
   until this job completes. Rich owns completion, with no CEO reminder required.

A commit whose release tooling predates the stable channel is refused at step 1, because
it cannot build a stable release of itself.

## Version numbers

The base is the next release version in `src-tauri/Cargo.toml`:

```text
1.2.0-nightly.20260921.42
1.2.0-nightly.20260921.43
1.2.0-nightly.20260922.44
1.3.0-nightly.20260923.45
```

The final number is one continuous candidate/build counter across **all dates and
base versions**. It never resets, is never padded and can grow beyond four digits.
The UTC date is captured at allocation after checks finish. Publishing stable `v1.2.0`
starts the next-version worker for `1.2.1` on main (step 8 above); the counter continues.

The number is reserved before building the candidate. Candidate 43 and published
nightly build 43 are the same build: `publish --run <run-id>` uploads the staged,
tested artifacts with their existing version, date and number. It does not rebuild
or allocate another number. A fresh build attempt gets a fresh number, even for
unchanged source (`--force` is needed if that source was already published).
Readiness checks and planning alone do not reserve numbers.

Failed builds and rejected candidates keep their numbers. If candidates 43 and 44
are rejected and 45 passes, installed users can go from build 42 directly to 45.
Gaps are expected. A rejected candidate keeps its number but never receives a
public nightly version tag or release-list entry.

Reservations are create-only remote refs at `refs/candidates/<number>`, pointing
to the candidate's build commit. They survive local state loss and machine changes.
An atomic push with an empty expected value refuses competing reservations, even
across dates and base versions or when a normal push could fast-forward the ref.
Retry a collision with a fresh plan. Never delete or overwrite a reservation to
reclaim a number. These refs are absent from branch, tag and release lists, but
remain publicly discoverable through Git and GitHub's API. They are not private.

Only `publish` creates `v<version>`, pointing to that same candidate commit, then
creates its GitHub prerelease. The rolling `nightly` channel tag remains separate.

Migration preserves all existing release versions and tags. The next number is
one greater than the larger of the number of distinct historical nightly version
tags and the highest number in any version tag or `refs/candidates/<number>`
reservation. Failed and unpublished attempts count too. Hidden reservations,
stable releases and the rolling channel entry do not add to the candidate count.
The 23 nightly candidates present at migration therefore seed **build 24**, even
though the largest old daily suffix was 8. Later reservations continue from there.
Old daily counters can contain repeated numbers; identify those legacy builds by
their full version. Newly allocated numbers are globally unique. A candidate
already staged before migration retains its original version when published.

If a Preview channel is introduced later, it should share this candidate counter
and its atomic reservations: nightly 24, preview 25, nightly 26. Each new candidate
gets its own number regardless of channel. Preview releases are not implemented yet.

## Existing local credentials

The same credentials as stable releases are used:

- Developer ID Application identity in the macOS Keychain.
- `~/.richos-signing/notary.env`, containing plain `RICHOS_NOTARY_KEY`,
  `RICHOS_NOTARY_KEY_ID` and `RICHOS_NOTARY_ISSUER` assignments. A
  `RICHOS_NOTARY_PROFILE` Keychain profile is also supported.
- `~/.richos-signing/richos-updater.key` and its `.pub` file.
- `~/.richos-privacy/named-persons`, the real release privacy-check list.
- Your configured Git identity, `git config user.name` and `user.email`.

The release commit and the channel commit are authored with this Mac's
configured Git identity rather than a synthetic `nightly@` one, because the release is
your own act and this machine's commit-identity guard refuses to push a commit that
says otherwise; `check` prints the identity it will author with and refuses in
milliseconds if none is configured instead of failing at the tag push after the gates.

The private files must belong to the current user and have mode 600. Their
contents are not copied to GitHub. `notary.env` is parsed as data, never executed
as a shell script. The check command signs a disposable local probe, checks Apple
notarization authentication and verifies the local updater public key matches the
app. It does not submit an app to Apple or create a GitHub release. The full
release path separately verifies the produced artifact's updater signature.

Credentials reach only the steps that use them: the preflight signing probe, the
notarization check, the updater signature and the publisher. The gates, meaning the
two `cargo test` runs, the twelve `app/scripts` suites and the privacy sweep, run in
an environment holding no `RICHOS_NOTARY_*`, `TAURI_SIGNING_*`, `APPLE_*`,
`RICHOS_NOTARIZE` or `RICHOS_SIGNING_IDENTITY` variable, including any the operator's
own shell exported. This is a correctness rule before it is a secrecy one: on
2026-09-16 the first nightly attempt published nothing because four
`package-app.test.sh` cases that refuse when notary credentials are absent or
half-supplied ran with a complete key in their environment.

Python 3.11+, Git, GitHub CLI, Rust, Tauri CLI 2.11.4, CMake (it builds the
runtime's `whisper-cli`) and Xcode command-line tools must be installed. GitHub CLI and Git must already be authenticated. A restricted
Keychain may require user interaction to sign; the command fails if its signing
probe cannot complete within 90 seconds. This implementation currently supports
Apple Silicon macOS, matching the bundled runtime recipe.

The runner verifies its public runtime cache before every build. The cache is
keyed to the recipe: `~/.richos-nightly/runtime-<first 12 hex digits of the
SHA-256 of runtime-sources.json>` (`scripts/lib/runtime_cache.py`). If it is
absent, the release command builds it from the pinned public source recipe, so a
changed recipe gets its own runtime and the previous one is no longer used. A
corrupted cache is a refusal. An existing verified runtime directory can be
selected with `--runtime-dir /absolute/path`. The check command validates a
present cache but never builds a missing one.

## Publication

The repository's `main-only` ruleset needs **no exception of any kind** and the
publisher reads it back before every publish to make sure it still has none:
`nightly.py check-rules` refuses the run unless `main` is the only branch exempt
from it, the ruleset is active, nobody can bypass it and both `creation` and
`update` are restricted. The runner asks the same question in preflight so a
widened rule costs seconds instead of forty minutes of gates.

That check exists because the opposite one used to. Until 2026-09-17 this page
asked for an exception for exactly `refs/heads/nightly-channel` and the runner
refused to start without one; on 2026-09-16 the exception was added and the next
morning the public repository page read "2 Branches". The channel is a release tag
now (below), so there is nothing left to grant.

No GitHub Actions secrets, environment or enabled CI workflows are needed.

Each explicitly triggered release runs `build` (steps 1-6) immediately
followed by `finish` (steps 7-8) — or, under the QA-gated path above, an
arbitrary amount of time can pass between the two, with a walker on the
candidate in between:

**`build`:**

1. Fetches and checks out a fixed remote `main` commit in the dedicated worktree.
2. Checks local signing, notarization and GitHub access.
3. Builds/verifies runtimes and runs core, updater, packaging and privacy gates.
4. Creates a child commit with the candidate's nightly version, lockfile entry,
   nightly endpoint and provenance, reserved by `refs/candidates/<number>`.
   It creates no public version tag and does not bump `main`.
5. Uploads or reuses the reproducible engine on the existing channel release under
   `richos-engine-sha256-<digest>.tar.gz`, then downloads and verifies it before
   compiling that URL and digest into the app. Reused assets are also verified.
6. Builds, signs, notarizes and staples the app locally. It verifies both the
   plist version and the executable's compiled update identity and records a
   SHA-256 of every staged file so `finish` can detect tampering later.

**`finish`** (what `publish --run <run-id>` invokes):

7. Refuses if any of the files `build` recorded no longer match what is on
   disk or if the built source commit is no longer an ancestor of the
   current `main`. Checks that the remote reservation still names this build and
   that remote main contains its exact approved changelog entry and candidate record.
   Missing, mismatched or stale notes refuse before any public version tag or app upload.
   Creates the public version tag and prerelease with those approved notes already
   in its body, then uploads immutable artifacts,
   provenance and checksums with the manifest last. Downloads them again to verify
   bytes and signatures. The engine URL compiled into the tested app is unchanged;
   an additional engine copy is included on the versioned release for its checksums.
8. Advances the update channel, in that order: a Git compare-and-swap push moves
   the rolling `nightly` release tag to a commit carrying the new `latest.json`
   and `build-info.json` and only then is that `latest.json` uploaded over the
   `nightly` release's asset, which is the file installed copies fetch. Older
   versions and regressed/divergent source commits are refused.

   The lease comes first deliberately. It is what decides who may publish, so a
   failure between the two steps leaves the tag naming a version whose asset was
   not replaced — installed copies simply stay on the previous nightly — and the
   command exits non-zero. Re-run `publish --run <run-id>`: it recognizes that the
   channel already records this exact candidate and re-uploads the asset without
   moving anything. The reverse order would put a build in front of users that no
   lease was ever taken for.

The release is marked prerelease and `--latest=false`, so stable updates are
unaffected. A partial publication is not offered to nightly users.
Existing tags and artifacts are never overwritten. Do not delete old release
assets automatically: installed apps may still need their pinned engine URLs.

If promotion stops after creating the tag or uploading some files, run `publish`
again for the same run ID. It accepts only a tag naming the tested commit, compares
existing remote assets with the staged bytes and uploads only missing assets.
Conflicting tags or bytes are refused, never overwritten. No rebuild or new number
is needed. A partially published release does not advance the update channel.

### Engine storage and retention

Candidate engines use stable digest-named URLs on the existing `nightly` release.
Never delete that release or overwrite or remove an engine asset an installable
build uses. Moving the channel tag or replacing `latest.json` does not remove them.
Candidates reuse identical archives after verifying the served bytes. A missing,
draft or immutable channel release is a refusal, not permission to create another.

GitHub permits at most 1,000 assets on one release. The runner reads the complete
paginated inventory and permits a new engine only if total assets after upload
remain at most 998, leaving two slots for channel metadata. Reuse consumes no slot
and is still allowed at capacity. Upload races never clobber an existing asset.
At capacity, stop and decide on approved overflow storage; no automatic deletion
or fallback creates a new visible release. The cap applies to distinct engine
archives, not candidate numbers. See [GitHub's release limits](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases#storage-and-bandwidth-quotas).

## Installing and leaving the channel

Installing a versioned nightly ZIP is the opt-in. Nightlies use:

```text
https://github.com/WebDevBooster/richos/releases/download/nightly/latest.json
```

That is an asset on `nightly`, a permanent prerelease whose tag moves on every
publish. It is pinned to that tag, so unlike the stable endpoint it does not
depend on which release GitHub considers `latest` and it puts nothing on the
public repository but a release tag.

**Copies installed before 2026-09-17 point at the retired
`raw.githubusercontent.com/.../nightly-channel/latest.json` and will not see any
further nightly through the updater.** An endpoint is compiled into the binary,
so it cannot be changed by an update that copy can no longer fetch. Replace those
copies by hand, once, with any nightly published from this change onward.

Stable builds keep their stable endpoint. For a user, a nightly replaces the same
app and uses the same data; there is no settings toggle and no second copy. The one
exception is a developer who has to keep his own copy untouched while he tests a
nightly beside it and that is the next section, not a product feature.
To leave the channel, install a stable version at least as new as the nightly.
Stable `1.2.0` supersedes `1.2.0-nightly.*`; stable `1.1.0` does not.

### Running a nightly beside your own copy: `scripts/nightly-launch.sh`

```sh
richos/app/scripts/nightly-launch.sh a ~/Downloads/RichOS-<version>-macos-aarch64.zip
richos/app/scripts/nightly-launch.sh b ~/Downloads/RichOS-<version>-macos-aarch64.zip
richos/app/scripts/nightly-launch.sh status
```

It unpacks the ZIP into `~/myrichos-nightly-a` (a newborn: empty data, what a
customer gets) or `~/myrichos-nightly-b` (the same nightly on data carrying the
team roster page), records the version and the ZIP's digest and opens that
folder's own app. Your own copy keeps your real `HOME` and `~/myrichos`. The
terminal can be closed once it prints the pid.

```text
~/myrichos-nightly-a/
  app.noindex/RichOS.app   the app, from the ZIP
  home.noindex/            the app's HOME: its data, engine, updates, temporary files
  memory/                  folder b only: wiki/team-roster.md
  logs/                    the app's output, one file per start
  nightly-launch.txt       version, commit and digest of the ZIP it came from
```

What makes it separate, all set by the script:

* **`HOME` and `CFFIXED_USER_HOME` are `<folder>/home.noindex`.** The app's
  data, its engine, its updater and its memory pointer all follow `HOME`. `HOME`
  alone is not enough: macOS's own frameworks still resolve the real home under
  it, so WebKit's store and the app's caches land in your `~/Library` under the
  same bundle identifier as your own copy. That was measured, not assumed: in a
  test guest an instance started with `HOME` alone wrote
  `~/Library/WebKit/com.richos.app` and `~/Library/Caches/com.richos.app` in the
  real home and instances started by this script wrote neither.
* **Nothing from the terminal it was started in.** `open` passes the caller's
  environment on to the app (also measured), so the script runs it under `env -i`
  with only what an app opened from Finder gets: launchd's `PATH`, who you are,
  your shell and ssh agent and a `TMPDIR` inside the folder.
* **The engine is the nightly's own.** `RICHOS_ENGINE_DIR` and
  `RICHOS_ENGINE_ROOT` are passed empty, never set: a published nightly boots only
  the engine installed from its own pinned asset and an explicit engine would
  outrank that pin. The first start installs it through the ordinary setup sheet.
* **Your Claude sign-in reaches it** through three things from your home: a link
  to `~/.claude`, a copy of `~/.claude.json` (refreshed at every start) and a
  link to `~/Library/Keychains`; plus a link to `~/.local/bin/claude`, which is
  where an app opened from Finder looks for `claude`. The cost is plain:
  `~/.claude` is your whole Claude configuration and history, shared with the
  terminal on purpose.
* `RICHOS_ACTIVATION=regular`, so the window comes to the front; `LORO_CORPUS`
  and `LORO_ROOT` empty, so the only memory is the folder's own.

Folder `b` gets `<folder>/memory/wiki/team-roster.md`, pointed at by its own
memory pointer. The page is read from `wiki/team-roster.md` in the record your
own copy already reads; `--roster <file>` names another. Until that page exists,
folder `b` refuses and says so.

It refuses a missing ZIP; a folder that holds another version unless you pass
`--replace` (which swaps the app and keeps the folder's data); a folder it did
not make; any path under `~/myrichos`, after following links; a second start
while that folder's nightly is running; and a `CLAUDE_CONFIG_DIR` anywhere the
app would inherit it.

* **To open it again**, run the same line or `nightly-launch.sh a --again`.
  A nightly that updated itself runs its updated copy from inside the folder.
* **To see what each folder holds**, `nightly-launch.sh status`: version,
  commit, what it updated itself to, whether it is running.
* **To get rid of one**, quit it and move the folder to the Trash. Nothing
  outside the folder refers to it.
* **Never open the nightly any other way.** Opened from Finder, from the Dock
  (keeping it there is the trap) or from the Apple menu's Recent Items, it runs
  on your real `HOME`, against your own copy's data and its updater installs
  into your real `~/Applications`. Both folders end in `.noindex` so Spotlight
  never offers it; the other ways in cannot be closed from here.
* **Only one of the two can serve your phone.** The phone listener binds fixed
  ports, so turning on phone pairing in a nightly collides with your own copy's.

What still leaves the folder, on purpose or because nothing can stop it:

* `~/.claude`, through its link: `claude` keeps its history and backups there,
  shared with your terminal. That is the price of the sign-in.
* WebKit's GPU and networking helpers keep a shader cache and blob files under
  macOS's own per-user folders (`getconf DARWIN_USER_CACHE_DIR` and
  `DARWIN_USER_TEMP_DIR`), in directories named for `com.richos.app`, which your
  own copy uses too. macOS starts those helpers itself, outside the app's
  environment, so no setting in this script reaches them. They hold caches, not
  your data.
* macOS's own bookkeeping of any app you open: the Recent Items list, Launch
  Services' registry.

## Going back when a nightly is bad

**Settings → Updates → "Go back to `<version>`".** One press. It fetches the
previous nightly's own manifest from that tag's release
(`releases/download/v<version>/latest.json`, which `finish` uploads beside the
archive and which is never overwritten), verifies its signature against the same
compiled public key as any update and stages it. The next time RichOS opens it
is running the earlier build — the same swap an update uses, at the same moment,
with nothing to do by hand. The boot line says which way it went:

```text
[richos] rollback activated: 1.2.0-nightly.20260920.2 -> 1.2.0-nightly.20260919.3
```

Four things it deliberately does not do, each of which is a refusal with one
sentence rather than a surprise:

* **It only ever goes to the version this copy actually came from**, read from
  the receipt the last update wrote before it swapped the bundles — not from a
  list of releases and not from anything the server says. A copy installed by
  hand has no such receipt and is told so.
* **It never steps up.** After going back, the version this copy came from is
  the newer one, so the control is gone; the way forward from there is an
  ordinary update and the newer nightly is still offered.
* **It refuses while an update is already prepared for the next launch.** That
  prepared update is what the next launch will do; open RichOS again first.
* **It is absent while RichOS is working**, exactly as the update control is.

Leaving the channel for good is still the paragraph above: a rollback moves
within the channel, it does not change which endpoint this copy fetches.

Proven end to end by `scripts/updater-e2e.sh` case R, on a fixture release
directory with two published tags and a moving channel tag.

## The release smoke: release-only steps run on every build

Every step a release performs and an ordinary day does not — the version written into
the manifest, the endpoint compiled into the binary, the candidate's recorded digests,
the updater metadata, his promotion record — runs on **every build**, against a
throwaway directory, before a single crate is compiled.

```sh
python3 richos/app/scripts/nightly.py release-smoke --out /tmp/some-throwaway-dir
```

It is `gates/release-smoke` and it is **first** because it is the cheapest refusal in
the build: **0.2 s** against roughly 950 s. The class it catches — a release-only step
that rotted since the last release — is otherwise found forty minutes in, by the
release that needed it. It is never skipped for a candidate and cannot be dropped by
`--checks-done-at-land`.

**One function, two callers.** The smoke does not re-implement anything. Each
release-only step is a single function that both the real release path and the smoke
call, marked `@release_step`. This is deliberately *not* how T3 Code does it: their
smoke duplicates their release workflow's inline bash and the two have already drifted
— `release.yml:795` passes two positional arguments in one order and
`release-smoke.ts:328-331` passes them in the other, so their smoke does not execute
the text their release executes.

**And the part that holds the line:** the smoke refuses unless **every** registered
step was reached during it. Add a release-only step and forget to smoke it and the
build fails naming your function. Without that check, this mechanism would decay into
T3's position one commit at a time with nothing red to show for it.

**Not in it, with the numbers.** Engine-asset packaging: `make-engine-asset.test.sh` is
112 s in the same build's `gates/script-suites` and the real build phase runs
`make-release.sh engine` for every candidate anyway — it is neither release-only nor
unexercised. Signing, notarization and upload need credentials and the network and a
gate gets neither by construction.

Its fixtures are removed however it ends, including when it refuses.

## Tests

```sh
bash richos/app/scripts/nightly.test.sh
bash richos/app/scripts/nightly-local.test.sh
bash richos/app/scripts/make-release.test.sh
cargo test --locked --manifest-path richos/app/crates/richos-user-update/Cargo.toml
```

Tests cover version ordering, reservations, source isolation, private credential
parsing, concurrent commands, explicit-only publication, failed checks leaving
the existing channel intact and the signing credentials never reaching a gate
subprocess. Local preflight is distinct from a complete signed release. Only an
explicit `publish` invocation makes the accepted, documented candidate installable.

Specifically for the `build`/`publish`/`candidate` split: `nightly.test.py`
asserts `build` stops before any app upload or channel move; that `finish`
refuses a candidate whose recorded files were tampered with, with a
positive-control test that the same untouched candidate publishes under the
same mocked harness; that `finish` refuses when the built source is no longer
an ancestor of `main` (a real force-push-to-orphan-history fixture); and that
the internal `nightly.py publish` helper reaches the same build/finish sequence
and channel state as separate calls. The controller does not use that fused helper:
`build`, `release` and `stable` record the candidate and queue notes before publication.
`nightly-local.test.py` asserts that wiring: `publish` recovers the notes request,
validates the recorded proof and invokes the current controller’s `finish` against
the immutable candidate checkout, without a signing credential;
`candidate` prints without running any command; and both `publish` and
`candidate` refuse a missing or unrecorded `--run <run-id>`.

For the release smoke and the stable channel: `nightly.test.py`'s `OneCodePathTests`
asserts that the smoke reaches every registered release-only step, that each of those
steps is named in the body of the code a real release runs (the failure it prevents is
not a rename — Python catches that — but a step re-implemented inline "just for this
path" while the smoke keeps exercising the old function) and that the smoke exercises
refusals rather than only happy paths. **The case that matters is the one that proves
the mechanism by defeat:** it registers a release-only step the smoke never calls and
requires the gate to turn red naming it. Without that case the completeness check could
be deleted and every other assertion would still pass.

`StableChannelTests` asserts that a stable plan resolves the nightly's source commit and
**that commit's** version rather than today's; that the build compiles in the stable
endpoint and carries no prerelease version anywhere (the two facts that make re-tagging a
nightly's bytes a broken stable release); that no decision of his — and a decision naming
a *different* nightly — means no stable release; that a version which already shipped
cannot ship again; and that publishing stable flips the release to `latest` in exactly
one call while leaving the nightly channel exactly where it was.

`nightly-local.test.py` asserts that `gates/release-smoke` is first in the gate order,
that `--checks-done-at-land` cannot drop it and that the gate environment is an
allowlist — including a case that plants a variable **no list in this repository
mentions** and requires no gate to see it. That case is the only one here that can tell
an allowlist from the deny-list it replaced; every other environment case names a
variable somebody already knew was dangerous and all of them passed against the
deny-list that broke the build on 2026-09-19.
