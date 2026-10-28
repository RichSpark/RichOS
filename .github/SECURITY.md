# Security policy

## Reporting a vulnerability

**Do not open a public issue for a security problem.** Use one of these two
routes instead.

1. **GitHub private vulnerability reporting — preferred.** Go to the
   [Security tab](https://github.com/RichSpark/RichOS/security) of this
   repository and choose **Report a vulnerability**. The report is visible only
   to you and the maintainer and GitHub gives you a private thread to discuss a
   fix in.

   If you do not see that button, the feature has not been switched on. It is a
   per-repository setting that has to be enabled by hand after a repository
   becomes public. Use email in that case and say so in your message so it gets
   enabled.

2. **Email: `webdevbooster@gmail.com`.** This address is already on every commit
   in this repository's public history, so writing to it discloses nothing new.
   Put `RichOS security` in the subject line.

   Encrypted mail is welcome but there is no published key, so ask for one first
   rather than sending ciphertext nobody can open.

There is no bug bounty. Nobody is paid for this and pretending otherwise would
be the first dishonest sentence in the file.

### What to put in the report

Whatever you have. A short description of the problem beats a long one that
waits for polish. If you can, include:

- what an attacker gets and what they need in order to get it;
- the version you looked at — a release tag such as `v1.0.2` or the commit
  SHA if you built from source;
- the steps or the proof-of-concept, if you have one;
- your operating system and how you built or installed RichOS.

### What to expect back

This is a one-person project. These are the numbers that can actually be kept,
which is why they are not shorter:

| Stage | Target |
|---|---|
| Acknowledgment that your report arrived | 5 business days |
| A first assessment — is it a vulnerability and how serious | 10 business days |
| A fix or a written explanation of why there will not be one | depends entirely on the finding and you will be told which |

If 10 business days pass in silence, send a follow-up. Silence here means a
message went astray, not that the report was judged and dismissed.

You will be credited by name in the commit and the release notes unless you ask
not to be. Please give the fix a reasonable amount of time to land before
publishing; if you have a disclosure deadline, say so in your first message and
it will be worked to rather than argued with.

## Supported versions

| Version | Supported |
|---|---|
| `main` | Yes |
| The newest release | Yes |
| Any earlier release | No — nothing is backported |

**Three releases exist — `v1.0.0`, `v1.0.1` and `v1.0.2`, all published on
2026-09-04 — and there are no maintenance branches.** A fix lands on `main` and
reaches everyone else in the next release, so the answer to "which version is
safe" is always the newest one. This table grows a real support policy the day
there is more than one line of development to support.

## Scope

### In scope

Anything in this repository that a RichOS user relies on:

- **The application** (`richos/app/`) — the Tauri shell, the `richos-core` runtime
  spine and the voice pipeline. The update path in `richos/app/src-tauri/src/updates.rs`
  is the highest-value target in the tree: it is what stands between a manifest
  URL and code running on somebody's Mac.
- **The engine** (`richos/engine/`) — hooks, guards, the installer and the skills it
  ships. A guard that can be made to pass while doing nothing is a real finding,
  not a cosmetic one.
- **The companion tools** (`richos/tools/`).
- **The packaging and signing path** (`richos/app/scripts/`).

Reports about the *absence* of a protection are welcome. So are reports about a
check that fires green while verifying nothing — that class of defect has cost
this project more than any exploit has.

### Known and deliberately unfinished

Reporting these is not useful, because they are already written down. They are
listed so nobody spends an evening on them.

- **No release exists yet for the updater to find.**
  `richos/app/src-tauri/tauri.conf.json` points at this repository's GitHub Releases,
  and until the first release is published that URL answers 404. Every
  downloaded byte is verified against a compiled-in minisign public key before
  anything is installed, so an update that cannot be verified is refused rather
  than applied.
- **Nothing is signed or notarized for distribution.** There is no release
  artifact. A build you make yourself is ad-hoc signed, which is why macOS
  re-prompts for microphone and accessibility permission after each rebuild.
- **Continuous integration may not have run on the commit you are reading.**
  Every workflow in `.github/workflows/` was disabled before publication, for
  the reason given in `.github/workflows/README.md` — which is the file that
  says what the current state is, so that two places cannot disagree about it.
  Check a commit's own checks rather than trusting a sentence here.
- **Personal machine paths appear throughout the verification evidence** under
  `docs/`. This was reviewed before publication and accepted: they are paths on
  one laptop, not credentials.

### Out of scope

- Findings in a third-party dependency that do not affect RichOS. Report those
  upstream. If one *does* affect RichOS, please report it here as well — the
  compiled dependency graph is inventoried in
  `docs/legal/THIRD-PARTY-RUST-DEPENDENCIES.md` and pinned by the two tracked
  `Cargo.lock` files, so a version claim can be checked against something real.
- Anything that requires the attacker to already have control of the machine
  RichOS runs on. RichOS is a desktop application; it does not defend against
  its own operating system.
- Results from an automated scanner with no analysis attached. A scanner
  finding, explained, is very welcome.

## How RichOS handles data, in one paragraph

RichOS is a desktop application that drives a locally installed Claude Code CLI
over stdio. Conversations, ledgers and session state are files on the user's own
machine. What leaves the machine is what the user's own AI provider receives
through that CLI, under that provider's terms. There is no RichOS server, no
telemetry endpoint and no account system and the feedback module in
`richos-core` carries test cases whose whole purpose is to prove that nothing
goes outbound from it.

Two outbound requests exist and are worth naming rather than leaving to be
found, because both fetch code that then runs:

- **First-run setup downloads Anthropic's own installer** from
  `https://claude.ai/install.sh` and runs it
  (`richos/app/crates/richos-core/src/setup.rs`). RichOS never rewrites, re-signs or
  nests that binary. The URL is a constant in the source; nothing chooses it at
  runtime.
- **The engine asset is fetched from a compile-time pin.** `RICHOS_ENGINE_URL`,
  `RICHOS_ENGINE_SHA256` and the version are baked in at build time, the
  download is refused unless its sha256 matches the pin exactly and a build
  with no pin does not fetch anything at all.

## Related documents

- `LICENSE` — GNU AGPL v3, the license for RichOS-authored software.
- `docs/legal/LICENSING.md` — what that license covers and the two carve-outs.
- `docs/legal/BRAND-ASSETS.md` — the brand and trademark exclusion.
- `docs/legal/THIRD-PARTY-NOTICES.md` — bundled third-party work and its terms.
- `docs/legal/THIRD-PARTY-RUST-DEPENDENCIES.md` — the compiled dependency
  inventory, keyed to the tracked lockfile digests.
- `.github/CONTRIBUTING.md` — how to contribute and what the AGPL means for a
  contributor.
