# Updating RichOS

RICH-TODOs row 12 said, verbatim:

> **There is no updater of any kind** — no `tauri-plugin-updater`, no Sparkle, nothing. The
> CEO's *"automatically download and install whatever the user needs"* currently rests on
> zero infrastructure and so does every future signed release.

This is what replaced it, what is proven and what is not.

---

## What is proven and how

Five different things are proven here and they are kept apart on purpose. A signing
identity, notarization, a stapled ticket, an installable public release and an update that
actually applied itself are five separate claims. Collapsing them into one cheerful sentence
is how a document of this kind starts being wrong.

### An installed RichOS updated itself over the internet — 2026-09-04

**This was the first time any copy of RichOS applied an update from a real endpoint.** Until
that afternoon the honest position, stated in this file, was that no update had ever crossed a
network.

A **v1.0.1 bundle downloaded from its own public release URL** was run with no endpoint
override in the environment, so it used the HTTPS endpoint compiled into it. It found 1.0.2,
downloaded the archive, **verified the minisign signature** and installed it:

```
RICHOS-UPDATE-SELFTEST state=available current=1.0.1 available=1.0.2 percent=- failure=- detail=-
RICHOS-UPDATE-SELFTEST state=ready current=1.0.1 available=1.0.2 percent=100 failure=- detail=-
RICHOS-UPDATE-SELFTEST exit=0
```

`ready` is reached only after the plugin verifies the downloaded bytes against the public key
compiled into the running build; a tampered or wrongly signed archive stops at the first
transition with `failure=signature`.

The bundle it left on disk then reported `CFBundleShortVersionString` **1.0.2**, `spctl`
**accepted / source=Notarized Developer ID**, `codesign --verify --deep --strict` valid and
its notarization ticket still stapled. Its code hash, `6afdf4b804ffef3a7cbb1568b2309ebfc130747f`,
is **identical to the independently downloaded 1.0.2 zip's** — which is what proves the
installed bytes are the published, notarized artifact rather than anything assembled locally.

A published 1.0.0 also sees 1.0.2. A published 1.0.2 reports `upToDate`. Raw output, exact
commands and the re-run recipe: `docs/verification/live-update-2026-09-04/`.

**What that run did not do is relaunch.** It exits at `ready`. Relaunch is proven by case D
below, on the local harness.

### The certificate and the three published releases

`security find-identity -v -p codesigning` reports **1 valid identity** —
`Developer ID Application: Alex Booster (TZ33A4QCZJ)`. Each of **v1.0.0, v1.0.1 and v1.0.2**,
downloaded from its public URL and assessed here, is signed by that identity with the hardened
runtime, **notarized**, **stapled** and `accepted` by Gatekeeper as
`source=Notarized Developer ID`. A quarantined 1.0.0 opened from Finder with no Gatekeeper
prompt of any kind (`docs/verification/first-run-as-a-stranger-2026-09-04/`).

`richos/app/scripts/make-release.sh` is the path that produces those: it signs `developer-id` and
notarizes by default and `--sign` and `--no-notarize` are the flags that turn either off, so
doing without them is a deliberate act. A bare `richos/app/scripts/package-app.sh` still signs
**ad-hoc**, which is correct for
a local build and is not what gets published.

### Six named cases on the local harness — 2026-08-31

**An update was applied end to end on this machine on 2026-08-31.** `richos/app/scripts/updater-e2e.sh`
builds RichOS 0.1.0 and 0.1.1, serves a manifest from a local HTTP server and makes the first
become the second — then flips one byte of the served archive and requires the install to
fail. Six named cases:

| Case | What it proves |
|---|---|
| A | 0.1.0 fetches the manifest over HTTP and finds 0.1.1 |
| B | it downloads the archive, the minisign signature **verifies** and it installs |
| C | the bundle on disk is now 0.1.1, read from the **installed** `Info.plist` |
| D | the replaced bundle **relaunches**, runs and reports itself as 0.1.1 and up to date |
| T | a **tampered** archive is refused with `failure.kind == "signature"` and the installed bundle is untouched |
| K | an archive signed by a **different key** is refused the same way |

T and K are the point as much as A–D. An updater that installs an unverified binary is worse
than no updater: it is a remote-code-execution channel with a progress bar. Neither case
asserts that the source *contains* a verification call — each one corrupts the artifact and
requires the install to **fail**.

The run's numbers are in `docs/verification/updater-e2e-2026-08-31/`.

#### What that harness run differs from a shipping build by, exactly

Two `--config` keys, both echoed by `package-app.sh` when it builds:

1. `version`, because the whole test is that two versions exist.
2. `plugins.updater.dangerousInsecureTransportProtocol: true`, because the manifest is served
   from `127.0.0.1` over http and a **release** build otherwise refuses a non-https endpoint
   outright (`tauri-plugin-updater-2.11.0/src/config.rs:validate_endpoints`). That refusal is
   correct and is **not** disabled in the shipping config.

The signing key, the public key, the archive format, the manifest shape, the signature check
and the install are the shipping ones. The transport is http instead of https and that is the
whole of the delta.

### The way back — case R, 2026-09-20

An updater that can only move forward is an updater with no answer to *"this one is bad"*.
The CEO runs the nightly as his daily driver and until this the only documented recovery was
`NIGHTLY.md`'s *"install a stable version at least as new as the nightly"* — by hand, from a
browser, on the machine he was trying to use.

**Settings → Updates → "Go back to `<version>`"** is now that answer and it is the update
path with two differences and no third:

1. **The manifest is one specific earlier tag's**, not the channel's.
   `scripts/nightly.py::finish` uploads `latest.json` to the per-version release as well as
   to the rolling channel release and existing tags and artifacts are never overwritten — so
   every published nightly carries its own immutable, signed manifest at
   `<host>/<owner>/<repo>/releases/download/v<version>/latest.json`. The URL is derived from
   the endpoint in force by replacing everything between `/releases/` and the final segment,
   which is the one rule that covers both the rolling nightly endpoint and the stable one.
2. **The version comparator.** The plugin's default is `remote.version > current_version`,
   which is exactly why a rollback cannot happen by accident: without the closure the fetch
   succeeds, the manifest parses and `check()` hands back `None`. It admits the one version
   this installation's own history named and nothing else.

Everything else is shared — including `Update::download`, which returns only after verifying
the payload against the compiled public key. **Going back is not a hole in the signature
story** and case R requires that rather than asserting it:

| Case | What it proves |
|---|---|
| R0 | the installation reached 0.1.1 through a real update, so it has a publication history |
| R1 | a **tampered** v0.1.0 archive is refused with `failure.kind == "signature"` — after the run reached the rollback and resolved 0.1.0, so the refusal is the signature's and not a run that never looked |
| R2 | the installed bundle is untouched by that refusal — still 0.1.1 |
| R3 | the intact v0.1.0 archive downloads, its signature **verifies** and the way back is staged |
| R4 | the running bundle is STILL 0.1.1 and the downgrade is authorized (`rollback.json`) but not applied — nothing swaps under a live session |
| R5 | the next ordinary launch relaunches into **0.1.0** and reports itself as 0.1.0 |
| R6 | the boot line names both versions and the direction: `[richos] rollback activated: 0.1.1 -> 0.1.0` |
| R7 | 0.1.1 is still offered afterwards — going back is reversible by an ordinary update, not a trap |
| R8 | there is no SECOND way back from 0.1.0, because the version it came from is newer; it says so instead of stepping up |

The fixture release directory is laid out the way a published one is —
`releases/download/{nightly,v0.1.1,v0.1.0}/` — because `previous_manifest_url` derives that
middle segment and a flat directory would have proved a rule the product does not use.

**The run: 19/19 on 2026-09-20**, A–D, T, K and R0–R8 together, on this machine. The three
lines that carry it, verbatim from that run — the refusal, the staging and what the copy
said about itself afterwards:

```
RICHOS-UPDATE-SELFTEST state=failed current=0.1.1 available=0.1.0 percent=100 back=0.1.0 rollback=true failure=signature detail=The signature verification failed
RICHOS-UPDATE-SELFTEST state=ready  current=0.1.1 available=0.1.0 percent=100 back=0.1.0 rollback=true failure=- detail=-
RICHOS-UPDATE-SELFTEST state=available current=0.1.0 available=0.1.1 percent=- back=- rollback=false failure=- detail=-
[richos] rollback activated: 0.1.1 -> 0.1.0
```

The tampered case flipped byte 12,165,992 of the 12,167,016-byte v0.1.0 archive — late in
the file, well past the gzip header, so it is a corrupted payload rather than an archive that
fails to open and the refusal is the signature's. The last selftest line is the copy running
as 0.1.0 with `back=-`: there is no second way back and 0.1.1 is offered instead.

**The selftest line grew two fields on this date** (`back=` and `rollback=`), so the
1.0.1 → 1.0.2 capture at the top of this file and the one in
`docs/verification/live-update-2026-09-04/`, are in the older format. They are captured
evidence of a run that happened and are left exactly as that run printed them.

---

## The signing key

Updates are signed with **minisign**, through `cargo tauri signer`. This has nothing to do
with Apple: Tauri verifies every downloaded byte against `plugins.updater.pubkey` before it
installs anything and that check is independent of codesigning. The two are separate
defenses and stay separate — minisign decides whether the archive is the one this project
signed, Apple's signature and notarization decide whether macOS will run the result. The
update path was provable before any certificate existed and it is the same path now that one
does; the certificate did not replace that check and does not weaken it.

**The public key is committed** (`richos/app/src-tauri/tauri.conf.json`, `plugins.updater.pubkey`).
It is compiled into every installed copy.

**The private key is never committed and never written anywhere inside either repository.**

```
# generate, OUTSIDE every git worktree
cargo tauri signer generate -w "$HOME/.richos-signing/richos-updater.key" -p '<password>'

# sign a release
export TAURI_SIGNING_PRIVATE_KEY_PATH="$HOME/.richos-signing/richos-updater.key"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD='<password>'
richos/app/scripts/package-app.sh --updater
```

`package-app.sh` **refuses** a key path inside a git worktree and **refuses** a key file
readable by other users of the Mac — the same two rules the Apple notary key already has. A
leaked updater key cannot be revoked: the public half is compiled into every copy already
installed, so the only remedy is a new key and a release nobody with the old build can reach.

### THE SHIPPING KEY, AND WHY IT WAS ROTATED BEFORE ANYTHING WAS INSTALLED

minisign key ID **`2F218991C928FD0A`**, generated 2026-09-04, private half at
`~/.richos-signing/richos-updater.key` (mode 600, in a 700 directory, outside every
repository). The public half is `plugins.updater.pubkey` and is compiled into every build.

It replaced key `A6BCB0F9A1ADED42`, whose private half is still on this Mac as
`richos-updater-TEST.key` and now signs nothing.

**Why it was rotated at all, when the two keys are cryptographically identical.** Because
the old one was called TEST and it was the default key of an automated harness that also
generates deliberately wrong keys to prove refusals. A file with that name and that job gets
deleted by somebody tidying up and deleting it is not recoverable: the public half is
compiled into every installed copy, so the only remedies are never shipping another update
or reinstalling by hand on every machine that has one. The name was a standing invitation to
destroy the one thing that can produce a valid update.

**Why now and not later.** Rotating costs a rebuild while RichOS is installed nowhere but
this Mac and one manual reinstall per machine after that. Today it was free. Every day it
is not.

**It has no password and that is a decision rather than an omission.** A password protects
the key file at rest — against a backup, a synced home directory, a paste into somewhere it
should not be — and protects nothing against anybody who can already read a mode 600 file on
this Mac. Where the password would live is the same question as how releases get made and
today they are made by hand, here. When releases move to CI that answer changes and so
should this. Treat adding a password as a NEW KEY PAIR unless something proves otherwise:
`cargo tauri signer` has exactly two subcommands, `generate` and `sign` and neither of them
re-encrypts an existing key.

**A leaked key cannot be revoked.** There is no revocation list and no expiry; the compiled-in
public key is the whole of the trust decision. A leak means a new key and a release that
nobody with an old build can reach.

```
# generate, OUTSIDE every git worktree
cargo tauri signer generate -w "$HOME/.richos-signing/richos-updater.key" -p ''

# sign a release
export TAURI_SIGNING_PRIVATE_KEY_PATH="$HOME/.richos-signing/richos-updater.key"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=''
richos/app/scripts/package-app.sh --updater
```

`package-app.sh` **refuses** a key path inside a git worktree and **refuses** a key file
readable by other users of the Mac — the same two rules the Apple notary key already has.

If the pair is ever replaced again:

1. `cargo tauri signer generate -w "$HOME/.richos-signing/richos-updater.key" -p '<password>'`
2. put the `.pub` contents into `plugins.updater.pubkey`
3. rebuild — and **every installed copy must be replaced by hand once**, because a copy
   carrying the old key refuses everything signed by the new one.

### One consequence of a `pubkey` existing at all, measured rather than assumed

A **raw** `cargo tauri build` — bypassing `package-app.sh` — now **fails** when no signing key
is in the environment. Measured on 2026-08-31:

```
$ env -u TAURI_SIGNING_PRIVATE_KEY -u TAURI_SIGNING_PRIVATE_KEY_PATH \
      cargo tauri build --bundles app
...
    Finished 1 bundle at:
        …/bundle/macos/RichOS.app
        …/bundle/macos/RichOS.app.tar.gz (updater)
       Error A public key has been found, but no private key. Make sure to set
             `TAURI_SIGNING_PRIVATE_KEY` environment variable.
$ echo $?
1
```

That is tauri-cli 2.11.4 `src/bundle.rs:277-279` and it fires **after** the bundle is built,
so the `.app` is on disk and the command still exits 1. It is not a defect — it is the CLI
refusing to publish an artifact it cannot sign.

`richos/app/scripts/package-app.sh` is unaffected: it passes `--no-sign` on every build, which on
macOS disables exactly that one step (codesigning is gated on Windows,
tauri-bundler `bundle.rs:301-306`) and then owns the signing itself. **Build through
`package-app.sh`.** `cargo tauri dev` is untouched — it does not bundle.

---

## The manifest

Tauri's updater fetches one static JSON document. `richos/app/updater/latest.example.json` holds the
exact shape; `package-app.sh --updater` writes the real one as `latest.json` beside the
archive.

```json
{
  "version": "0.1.1",
  "notes": "Faster launch, and the settings menu now says what version you are running.",
  "pub_date": "2026-08-31T21:04:11Z",
  "platforms": {
    "darwin-aarch64": {
      "signature": "<the whole contents of RichOS.app.tar.gz.sig>",
      "url": "https://github.com/WebDevBooster/richos/releases/download/v0.1.1/RichOS.app.tar.gz"
    }
  }
}
```

* The platform key is `{os}-{arch}` — `darwin-aarch64` on Apple silicon, `darwin-x86_64` on
  Intel (`tauri-plugin-updater-2.11.0/src/updater.rs:updater_os`/`updater_arch`). An Intel
  build needs its own entry; the same manifest can carry both.
* `signature` is the **whole** `.sig` file, base64, on one line — not a hash and not a
  fragment of one.
* `version` is compared with semver against the running version. A manifest offering a version
  that is not strictly greater produces `upToDate` and no download.
* The updater sends `Accept: application/json` and treats **`204 No Content` as "no update"**,
  which is the correct answer for an endpoint that wants to say "not yet" without a body.
* `notes` is shown to the CEO verbatim, in the settings menu's Updates row. It is never
  generated from a commit log: a changelog is not a sentence a non-technical reader can act on.

### `{{target}}`, `{{arch}}`, `{{current_version}}`

The endpoint may carry those placeholders and the plugin substitutes them, which is what lets
one URL serve a per-platform document. The committed endpoint uses them, so a static host that
serves a file tree works without a server.

---

## Where updates are hosted — DECIDED 2026-09-04: GitHub Releases

The endpoints committed in `tauri.conf.json` are, in the order they are asked,

```
https://github.com/RichCore/RichOS/releases/latest/download/latest.json
https://github.com/WebDevBooster/richos/releases/latest/download/latest.json
```

**Two addresses, for the GitHub move (CEO, 2026-10-27).** The repository moves to
`RichCore/RichOS`. `updates.rs` (`first_answer`) asks the new address first and the old one
second and the first that answers with a manifest wins. Before the move the new address does
not exist, so the old one answers. After the move the new one answers and the old one is not
asked. A download that a manifest still names on the old address is fetched from the new one.
The steps of the move are in the private record:
`richos-hq/docs/operations/2026-10-27-github-richcore-move.md`.

One static document, on the host the repository already lives on, reached through GitHub's
`latest` pointer so the URL compiled into every installed copy never has to change again.
`richos/app/RELEASING.md` is the procedure; what follows is why this URL and not another.

**`{{target}}`, `{{arch}}` and `{{current_version}}` are gone from the endpoint, deliberately.**
The plugin substitutes them into the URL (`updater.rs:475-487`), which is what lets one
endpoint address a per-platform file tree — and GitHub Releases is a flat asset store that
templates nothing, so a path built from them would simply 404. The per-platform split moves
into the document instead: `RemoteRelease` has two shapes and the STATIC one is a
`platforms` map keyed by `{os}-{arch}` (`updater.rs:79-86`, `download_url`/`signature`).
One manifest carries every platform and an Intel entry is a key beside the Apple silicon
one rather than a second URL.

**The redirect chain resolves and it was measured rather than assumed.** GitHub answers
`/releases/latest/download/<asset>` with a 302 to `/releases/download/<tag>/<asset>`, which
redirects again to its asset CDN. Measured 2026-09-04 against a public Tauri project that
publishes exactly this file:

```
$ curl -sSL -o /dev/null -w '%{http_code} %{num_redirects} %{content_type}\n' \
       https://github.com/ayangweb/EcoPaste/releases/latest/download/latest.json
200 3 application/octet-stream
```

Three redirects and `reqwest`'s default policy — which the plugin never overrides — follows
up to ten (`reqwest-0.13.4/src/redirect.rs:160-165`). The `Content-Type` comes back as
`application/octet-stream` rather than `application/json`, which does not matter: the plugin
reads the body with `Response::json` and that deserializes the bytes without ever looking at
the content type (`reqwest-0.13.4/src/async_impl/response.rs:269-273`). Both of those are the
kind of thing a copied blog pattern gets right by luck.

That measurement borrowed another project's file because RichOS had published nothing yet.
**It has since been repeated against RichOS's own endpoint**, which now answers:

```
$ curl -sSL -o /dev/null -w '%{http_code} %{num_redirects} %{content_type}\n' \
       https://github.com/WebDevBooster/richos/releases/latest/download/latest.json
200 2 application/octet-stream
```

Two redirects rather than three, the same content type and the document that comes back names
version 1.0.2. The endpoint compiled into every installed copy resolves.

**The failure mode this shape has, stated plainly.** `latest/download/<asset>` resolves
against whatever GitHub currently calls the latest release and it 404s when that release
does not carry the asset. The same measurement found two live projects in exactly that
state — their newest release has no `latest.json`, so their updater endpoint answers 404 for
every installed copy. So:

* every release that ships an application MUST carry `latest.json` and it is uploaded LAST,
  after the archive it names is already reachable;
* a release that ships something else — an engine asset on its own, say — must be marked
  pre-release or it becomes "latest" and takes the manifest away from everybody.

`richos/app/RELEASING.md` enforces both in the script rather than in a person's memory.

**A 404 is not `unconfigured`.** When no release carries the manifest, a check reports a
`manifest` failure, because that is what the plugin returns (`Error::ReleaseNotFound`,
classified in `updates.rs`). The `unconfigured` state only appears when a build is
deliberately pointed at the `.invalid` host through `RICHOS_UPDATE_ENDPOINT`. Releases now
exist and the endpoint answers 200, so neither state is what anybody sees today — which is
exactly why the two bullets above are printed as ordered commands by `make-release.sh` and
re-checked afterwards by its `verify-release` subcommand, rather than remembered.

**What was NOT chosen and why it may still be right later.** A CDN bucket or a small
endpoint on existing infrastructure both buy things GitHub Releases cannot: a `204 No Content`
answer for "not yet" and a staged rollout to one machine before everybody. Neither is worth
a domain, a bill and a second thing to keep alive for a customer count of one. The
engineering is indifferent between them — all three serve one JSON document and one
`.tar.gz` — so this is reversible for the price of one config line and one rebuild.

## What the CEO sees

**When an update is waiting, a small pill appears beside the universal settings button reading
*RichOS 1.0.2 is available*.** It is not there the rest of the time — not dimmed, not saying "up
to date", not there at all — so what the CEO notices is a thing arriving in chrome he is already
looking at rather than a control he has learned to skip. Pressing it opens the Updates row below
and puts his hand on that row's own button. It never installs anything itself.

That placement is CEO ruling §26's placement paragraph, given 2026-09-04 in these words: *"the
user can't be bothered to hunt for some update button somewhere."* Until then the row below was
the whole surface and the row lives inside a menu — so an available update was discoverable only
by someone who happened to open that menu and nothing gave them a reason to.

The settings button also carries a **mark** when an update is waiting or installed. Opening it
shows an Updates row with, in every case, a sentence. The version numbers below are the ones
the interface tests use as fixtures, not the shipping series — the published releases are
1.0.0, 1.0.1 and 1.0.2:

* *RichOS 0.1.0 is up to date.* / *Checked 8 minutes ago.*
* *RichOS 0.1.1 is available.* + the release notes + **Download and install**
* a progress bar, then *Checking the download and installing…*
* *RichOS 0.1.1 is installed.* / *Restart when you are ready — nothing is lost.* + **Restart to finish**
* every failure in its own words, with the vendor's own error text one click behind it

**RichOS checks by itself, once, three seconds after launch** (`updates.rs`,
`spawn_launch_check`). Once per launch, not on a timer and it downloads nothing — so a
machine that is never restarted never checks again. **The install is a button the CEO presses,
and a relaunch is needed after it.** That is
deliberate: on macOS the installer deletes and replaces the running `.app` in place and RichOS
holds a `claude-agent-acp` child process as a compute lease that the session-continuity design
forbids swapping mid-turn (the session-continuity design record,
`richos-session-continuity-2026-08-24.md` §3.1, which is kept privately in richos-hq and
is not part of this repository). Doing
that behind the CEO's back is that invariant broken by a background thread.

**A refused signature is never offered a retry.** Every other failure gets *Try again*; a
signature failure gets an explanation, because retrying a tampered artifact refuses
identically and a button that invites someone to keep pressing until a security check passes is
the wrong control.

**And when a version turns out to be bad, there is a way back in the same row.** A control
reading *Go back to 0.1.0* — the plain button, not the gold one, because going back is not
the row's recommended act. It names its destination, because *Go back* on its own is a
control nobody presses. It is absent, not dimmed, wherever pressing it would not be true:
while RichOS is working, once something is already prepared for the next launch and when
this copy holds no record of an earlier version it came from. Afterwards the row says
*RichOS will go back to 0.1.0 when you next open it* and the newer version is still offered,
so the way back is not a one-way door.

**When nothing is running, RichOS restarts into the update by itself (CEO feedback
2026-10-06, item 1).** His words: *"the app should automatically re-launch immediately after
downloading the new version. Avoiding/preventing a re-launch is only meant for when there are
workers running and an immediate relaunch would destroy their work."* So once an update this
copy downloaded is prepared and the work gate is clear, the row and the pill say *Restarting
into RichOS 1.2.1…* for three seconds, RichOS quits through its own quit path and a small
helper (`src-tauri/src/update_relaunch.rs`) starts it again the moment the old process and its
session lease are gone, which is what lets the new start activate the update. The window, the
open conversation and an unsent draft come back where they were. While a turn, a worker or
background work runs, nothing changes from before except the sentence: *RichOS will restart
into this update as soon as that work is done* and the watcher relaunches within five seconds
of the work ending. An update prepared by an earlier launch that could not activate it keeps
the next-launch sentence, so a relaunch can never loop.

---

## What is NOT proven, in those words

* **Windows is unproven.** `plugins.updater.windows.installMode` is configured and no Windows
  bundle has ever been built for RichOS — that needs a .NET toolchain this machine does not
  have (RICH-TODOs row 8) and there is no Windows code-signing certificate (the packaging and
  signing record, kept privately in richos-hq and not part of this repository). Nothing here
  says Windows works.
* **Nothing has relaunched after a REAL update.** The update proven over the internet on
  2026-09-04 stops at `ready`; the self-test exits there by design. Relaunch is proven only by
  case D on the local harness, on locally built 0.1.x bundles. Nobody has watched a published
  bundle replace itself and come back as the new version.
* **No update has been applied to a translocated copy.** The 2026-09-04 run used a copy in an
  ordinary directory with no `com.apple.quarantine` attribute.
  `docs/verification/first-run-as-a-stranger-2026-09-04/` observed a genuinely quarantined
  1.0.0 running **App-Translocated** from a read-only image, which is where a person's first
  download actually runs from until they move it. Whether an in-place update succeeds from
  there is not measured, in either direction.
* **NO GRANT HAS BEEN WATCHED SURVIVING AN UPDATE.** What is measured is the mechanism and it
  is worth stating exactly, because the opposite used to be true and this document said so.
  Under Developer ID the designated requirement is
  `identifier "com.richos.app" … certificate leaf[subject.OU] = TZ33A4QCZJ`, character for
  character identical across v1.0.0, v1.0.1 and v1.0.2, with **no `cdhash` term in it** — so
  it is not a hash of one build and a new build signed by the same certificate satisfies it.
  The microphone row in this Mac's TCC database is bound to that same hash-free requirement.
  Both facts are in `docs/verification/live-update-2026-09-04/`. **What is missing is the
  outcome**: nobody has granted the microphone to one version, updated and used the
  microphone again without being asked. Accessibility has no row at all, so there is nothing
  there to survive yet. Ad-hoc bundles, whose requirement *is* a code hash, do still lose both
  grants on every rebuild — that is why releases are signed with the certificate.
* **Rollback does not exist.** A bad release is fixed by publishing a newer one. The updater
  compares semver and will not install a lower version.
* **There is no staged rollout and no channel.** One manifest, everyone gets it.
* **A FAILURE IS STILL QUIET IF THE MENU IS SHUT.** The cue announces the two states that are
  waiting on the CEO — available and installed — and deliberately not `failed`. The failure
  headlines above are whole sentences and a pill wide enough to carry one is the banner §26
  asks for the opposite of. So a refused signature or a dropped connection is stated in full in
  the Updates row, exactly as it was before the cue existed and says nothing in the chrome.
  Nothing regressed; nothing improved either. **What a failure should look like in chrome is
  one of the things the CEO's own reference does not answer** — that capture is a success path
  only.
* **Mode 1 does not exist.** §26 makes fully automatic updating the default and the CEO ruled
  on 2026-09-04 that it waits for a design session of its own. What ships is mode 2 — confirm
  each time — with the confirmation now reachable in one press from chrome rather than from
  inside a menu. Nothing in the cue presupposes mode 1.

---

## The Developer ID certificate — what it closed and what it did not

**This section used to say the certificate did not exist.** It said
`security find-identity -v -p codesigning` reported 0 valid identities, that every update
shipped an ad-hoc bundle, that microphone and accessibility grants died on every update and
that nothing was notarized. **Every one of those was true when it was written and none of them
has been true since 2026-09-01.** It is left named rather than quietly deleted because a
document that has been wrong should say where.

What is true now, one claim at a time:

* **The Apple Developer Program membership exists** (2026-08-31; CEO decision 1.1 closed).
* **The Developer ID Application certificate exists.** `security find-identity -v -p codesigning`
  reports **1 valid identity**, `Developer ID Application: Alex Booster (TZ33A4QCZJ)`, valid to
  2031-09-02.
* **Releases are signed with it, with the hardened runtime, then notarized and stapled.**
  `make-release.sh` does all four by default and `verify-release` re-downloads what was
  published and checks it again.
* **Gatekeeper accepts a downloaded copy.** All three published zips assess as
  `accepted / source=Notarized Developer ID` and a quarantined 1.0.0 opened from Finder with
  no prompt at all. There is no right-click-Open step for a person installing RichOS.
* **The identity no longer changes per build.** The designated requirement names the bundle
  identifier and the certificate and carries no `cdhash` term — so macOS treats every build
  signed with this certificate as the same application. That is the mechanism grants depend
  on and it is measured on all three releases.

What the certificate did **not** close and is listed again here so a reader who skipped the
section above does not leave with the wrong impression:

* **No microphone or accessibility grant has been observed surviving an update.** The
  mechanism is measured; the outcome is not. Accessibility has never been granted to RichOS at
  all, so it has nothing to lose yet.
* **Windows has no certificate and no bundle.** Nothing about this identity touches it.

The two scripts that produced the certificate are still in the tree, for the day it is renewed
or replaced: `richos/app/scripts/make-signing-csr.sh`, then the Apple portal, then
`richos/app/scripts/install-signing-cert.sh <the .cer>`. `docs/ceo/developer-id-setup-2026-08-31.md`
is the middle step in plain language. The rebuild-identity measurement that settled the grant
mechanism is `docs/verification/rebuild-survival-2026-09-01.md`.

---

## Running it

```
# the whole end-to-end, about the length of two release builds
richos/app/scripts/updater-e2e.sh

# ...and keep the workspace (bundles, logs, served manifest) for inspection
richos/app/scripts/updater-e2e.sh --keep

# minisign an update archive from an ordinary packaging run.
# This is a LOCAL build: ad-hoc codesigned, not notarized, not publishable.
TAURI_SIGNING_PRIVATE_KEY_PATH=$HOME/.richos-signing/richos-updater.key \
TAURI_SIGNING_PRIVATE_KEY_PASSWORD= \
RICHOS_UPDATE_BASE_URL=https://<host>/richos/0.1.1 \
RICHOS_UPDATE_NOTES='What changed, in the CEO's language.' \
  richos/app/scripts/package-app.sh --updater

# a REAL release — Developer ID signed, notarized, stapled, minisigned, staged.
# It is a chain of subcommands in a forced order, and richos/app/RELEASING.md is the procedure:
richos/app/scripts/make-release.sh plan            --tag v1.0.3
richos/app/scripts/make-release.sh app             --tag v1.0.3 --notes 'What changed, in his language.'
richos/app/scripts/make-release.sh verify-release  --tag v1.0.3   # after the assets are uploaded
```

Without `RICHOS_UPDATE_BASE_URL` the artifacts are still built, verified and minisigned and
**no manifest is written** — a manifest carrying a guessed URL is worse than no manifest,
because it is a file that looks publishable.
