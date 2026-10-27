# Releasing RichOS

One release, start to finish, with nothing to reconstruct. Everything below is run by
`richos/app/scripts/make-release.sh`; this page is why each step exists and what it refuses.

For manually triggered local nightly builds and channel setup, see [NIGHTLY.md](NIGHTLY.md).

If you only read one thing: **the order is forced and `latest.json` is uploaded last.**

---

## What a release is

Five files, on one GitHub release, under one tag:

| Asset | What it is |
|---|---|
| `RichOS-<version>-macos-<arch>.zip` | the FIRST install — a Developer ID signed, notarized, stapled `RichOS.app`, archived with `ditto` |
| `RichOS.app.tar.gz` + `.sig` | the UPDATE — the same bundle in the archive shape the updater installs, minisigned |
| `latest.json` | the manifest every installed copy fetches |
| `richos-engine-<engine version>.tar.gz` | the engine, pinned by digest inside the app |
| `SHA256SUMS` | every one of the above, in the format `shasum -c` reads |

The tag is `v<version>` and the version comes from `richos/app/src-tauri/Cargo.toml` and nowhere
else. `tauri.conf.json` deliberately has no `version` key: it would override the Cargo
manifest (tauri-utils 2.9.3, `config.rs:3612` — *"If removed the version number from
`Cargo.toml` is used"*), leaving two places to edit and one that wins silently. Everything
downstream derives: the tag, the URLs, the archive name and the manifest's own `version`,
which `package-app.sh` reads back off the produced bundle's `Info.plist` rather than off any
config.

**Nothing at the repository root.** The root is nine entries by the CEO's deliberate design.
Release artifacts are staged under `richos/app/target/release-staging/<tag>/`, which is ignored and
published to GitHub — never committed.

---

## The order and why it cannot be rearranged

```
1.  make-release.sh engine          build the engine asset, prove it reproducible, write the pin
2.  gh release create               the tag exists
3.  gh release upload  <engine>     the engine asset is reachable
4.  make-release.sh verify-engine   download it and require the pinned digest
5.  make-release.sh app             compile the verified pin in; sign, notarize, staple
6.  gh release upload  <app assets, SHA256SUMS>
7.  gh release upload  latest.json  LAST
8.  make-release.sh verify-release  download everything and require it to match
```

**The engine goes first because the app carries its URL and SHA-256 as compile-time
constants.** `richos-core/src/setup.rs::engine_pin` reads three `option_env!` values, so the
digest lives inside the executable that the Developer ID signature covers. A runtime variable
would be a value an attacker on the machine could set, which is the opposite of a pin. That
also means the digest has to be computed from the bytes that will *actually be served* —
`verify-engine` downloads them and `app` refuses to build until that receipt exists and
matches the current pin.

**`latest.json` goes last because the endpoint is a moving pointer.** Every installed copy
fetches

```
https://github.com/WebDevBooster/richos/releases/latest/download/latest.json
```

(or the same path at `RichSpark/RichOS`, which a copy carrying the GitHub move work asks first;
`UPDATES.md`) and GitHub resolves `latest` to whatever the newest release is. From the moment this release
becomes the newest, that manifest is what everybody gets — so it must not exist before the
archive it names does. A manifest naming a file that is not there yet is an update that fails
for every customer, at the one moment nobody is watching.

**An engine-only release must be marked pre-release.** Otherwise it becomes "latest" and takes
`latest.json` away from every installed copy and the endpoint answers 404. This is not
hypothetical: on 2026-09-04 two live public Tauri projects were measured in exactly that
state — their newest release carries no `latest.json`, so their updater endpoint answers 404
to every copy they have shipped.

That `pre-release` marking is load bearing, so it was checked rather than assumed. Measured
2026-09-04 against `neovim/neovim`, which publishes a `nightly` pre-release every day:

```
releases[0]  nightly  prerelease=true   published 2026-09-04T05:25:34Z
releases[1]  v0.12.5  prerelease=false  published 2026-08-23T18:27:13Z

releases/latest -> v0.12.5
```

`latest` skipped a release published twelve days later because it was marked pre-release. It
resolves to the most recently published release that is neither a draft nor a pre-release.

## Why the first install is a zip and not a disk image

`cargo tauri build --bundles app,dmg` creates the disk image DURING the build — which is
before `package-app.sh` signs the bundle, notarizes it and staples the ticket. The image would
therefore carry a bundle from before all three and the image itself would be neither signed
nor notarized. Publishing it would hand a stranger a download that Gatekeeper refuses, for
reasons that have nothing to do with anything being wrong with the application.

The `ditto` archive is made after stapling, from the finished bundle and the ticket rides
inside it — so the first launch works on a machine with no network. `make-release.sh app`
extracts the archive it just wrote and runs `codesign --verify --deep --strict`,
`xcrun stapler validate` and `spctl` on the EXTRACTED copy, because that copy is what the
person downloading it actually gets.

---

## Run it

```bash
# 0. what this release will be. Touches nothing.
richos/app/scripts/make-release.sh plan

# 1. the engine asset + the pin, with reproducibility proved
richos/app/scripts/make-release.sh engine

# 2-3. publish the engine asset first
gh release create v0.1.0 --repo WebDevBooster/richos \
    --title 'RichOS 0.1.0' --notes 'What changed, in the language the reader speaks.'
gh release upload v0.1.0 richos/app/target/release-staging/v0.1.0/richos-engine-1.0.0.tar.gz \
    --repo WebDevBooster/richos

# 4. the digest, off the wire
richos/app/scripts/make-release.sh verify-engine

# 5. the app, against the verified pin
export TAURI_SIGNING_PRIVATE_KEY_PATH="$HOME/.richos-signing/richos-updater.key"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=''
export RICHOS_NOTARY_KEY=...        # see richos/app/scripts/package-app.sh --help
richos/app/scripts/make-release.sh app --notes 'What changed, in the language the reader speaks.'

# 6-7. the app assets, then the manifest LAST
gh release upload v0.1.0 --repo WebDevBooster/richos \
    richos/app/target/release-staging/v0.1.0/RichOS.app.tar.gz \
    richos/app/target/release-staging/v0.1.0/RichOS.app.tar.gz.sig \
    richos/app/target/release-staging/v0.1.0/RichOS-0.1.0-macos-aarch64.zip \
    richos/app/target/release-staging/v0.1.0/SHA256SUMS
gh release upload v0.1.0 --repo WebDevBooster/richos \
    richos/app/target/release-staging/v0.1.0/latest.json

# 8. the loop closes: download everything published and require it to match
richos/app/scripts/make-release.sh verify-release
```

`make-release.sh` never uploads and never creates a release. It prints the exact commands
with the paths filled in and the only two things it does over the network are downloads.

---

## What it refuses and what each refusal is protecting

| It refuses when | Because |
|---|---|
| `tauri.conf.json` carries a `version` | the version would be written in two places and the wrong one could win |
| the configured updater endpoint is not the one this release publishes to | every claim in the run would be about a file nobody fetches |
| the engine asset is not deterministic | the digest compiled into the app would be a property of the machine that built it and a customer would discover that as a `DigestMismatch` |
| the published engine asset differs from the pin | the failure would ship inside a signed binary |
| `app` runs with no `verify-engine` receipt | the digest would be a claim about a local file |
| the working tree has uncommitted changes | the binary would not correspond to the source published beside it and nobody can reconstruct what shipped from a tree that was never committed |
| the tag exists locally and points somewhere other than `HEAD` | GitHub publishes the tag's tree as the source beside the release |
| the digest is not found inside the built executable | the pin never reached the compiler, so the app would install whatever the URL returns |
| the built bundle's version is not the release version | the tag, the manifest and the binary would disagree |
| the extracted first-install archive fails `codesign` or has no stapled ticket | a machine that is offline treats it as un-notarized, which for that person is the same as not being notarized |
| the manifest does not satisfy `tauri_plugin_updater::RemoteRelease` | the customer's own parse would fail the same way |
| the manifest's `signature` is not the `.sig` beside the archive | it is well formed, publishable and refused by every installation |
| `SHA256SUMS` does not verify against the files it names | a checksum file that was written is not a checksum file that is right |

Every one of those is checked against an artifact. None of them is an exit code.

---

## The signing key

Updates are signed with **minisign**, key ID `2F218991C928FD0A`, private half at
`~/.richos-signing/richos-updater.key` (mode 600, in a 700 directory, outside every
repository). The public half is compiled into every build as `plugins.updater.pubkey`.

This is unrelated to Apple. Tauri verifies every downloaded byte against that key before it
installs anything and that check holds whether or not the bundle is codesigned — which is why
the whole update path was provable before a certificate existed.

**A leaked or lost key cannot be revoked.** There is no revocation list and no expiry. A new
key means a release that nobody with an old build can reach and one manual reinstall per
machine. `richos/app/UPDATES.md` records why the key was rotated on 2026-09-04 and why it has no
password.

`package-app.sh` refuses a key path inside a git worktree and refuses a key file readable by
other users of the Mac.

---

## The source corresponding to the binary

RichOS is AGPL-3.0-only and the release page is where a stranger goes looking for the source
that made the download. GitHub attaches the tag's tree to every release as `Source code (zip)`
and `Source code (tar.gz)` automatically, so nothing has to be uploaded for it — but it is only
*corresponding* source if the build came from that exact tree.

`make-release.sh app` therefore refuses a dirty working tree outright and refuses to build when
the tag already exists locally and points at a different commit than `HEAD`. It prints the SHA it
is building and says so plainly when the tag does not exist yet: create it on that commit or
the source published beside the release is a different tree from the one that shipped.

`SHA256SUMS` covers the assets this project uploads. It does not cover GitHub's two generated
source archives, which GitHub creates on demand and which are not byte-stable over time — the
tag is their identity and it is a stronger one than a checksum of a file regenerated on request.

## After publishing

Run `make-release.sh verify-release`. It downloads every asset from its published URL,
compares each against `SHA256SUMS`, fetches the updater endpoint the way an installed copy
does and hands the fetched manifest and the fetched archive to the same
`verify_update_manifest` check. Until that passes, the release is a set of files somebody
uploaded — not a release anybody can install.

The one thing it cannot tell you is whether an installed copy on another machine applies the
update. That is `richos/app/scripts/updater-e2e.sh`, which builds two versions and makes one become
the other and it is the only place that claim is ever made.

---

## Adding a second architecture

The manifest's `platforms` map is keyed `{os}-{arch}` and one document carries every platform
(`tauri-plugin-updater-2.11.0/src/updater.rs:79-86`). An Intel build adds a `darwin-x86_64`
entry beside `darwin-aarch64`; it does not need a second manifest, a second endpoint or a
template in the URL. `package-app.sh` writes the entry for the machine it runs on, so a
two-architecture release merges two manifests before the upload — and `verify_update_manifest`
should be run once per architecture, on the machine that built it.

Windows is not covered here. No Windows bundle has ever been produced and there is no Windows
code-signing certificate; `richos/app/UPDATES.md` says so in those words.
