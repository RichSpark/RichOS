# Licensing

**RichOS-authored software is licensed under the GNU Affero General Public
License, version 3 and no later version — SPDX `AGPL-3.0-only`.**

The canonical text is the file `LICENSE` at the root of this repository. It is
byte-identical to the Free Software Foundation's published text
(<https://www.gnu.org/licenses/agpl-3.0.txt>), sha256
`0d96a4ff68ad6d4b6f1f30f713b18d5184912ba8dd389f86aa7710db079abcb0` and it is
never to be edited. It sits at the root because GitHub's license detection reads
only the repository root; a `LICENSE` moved anywhere else silently loses the
badge that tells a visitor what they may do.

**That file is the license. This file explains its scope.** Where the two could
ever be read as disagreeing, the `LICENSE` text governs — a scope note cannot
enlarge or shrink a grant, only describe where it applies.

## What "AGPL-3.0-only" means here

You may use, study, modify and redistribute this software. In exchange:

- **Source travels with the software.** Anyone you give a binary to can demand
  the corresponding source.
- **Section 13 — the network clause.** If you run a *modified* version and let
  other people interact with it over a network, you must offer those users the
  corresponding source of your modified version. This is the term a permissive
  license does not have and it is why this license was chosen: RichOS is the
  kind of software somebody would otherwise host as a closed service.
- **`-only`, not `-or-later`.** The grant is version 3 of the AGPL and nothing
  else. A future FSF version does not apply automatically.

Copyright remains the owner's. The AGPL binds everyone who *receives* the
software under it; it does not bind the copyright holder, who may separately
offer the same code under commercial terms to anyone who does not want the
AGPL's obligations.

## What the AGPL does NOT cover

Two carve-outs, both narrow, both written down file by file.

### 1. Brand material

The RichOS name, the mark and wordmark, the application icons, the banner
artwork, the Rich Hand avatar and the RichPass name and its logo and icon
artwork are **not** licensed under the AGPL and remain all rights reserved
unless separately authorized.

`docs/legal/BRAND-ASSETS.md` is the governing document. It names every excluded
file and directory exactly, names the three source constants that carry the
mark's geometry, says what a visitor may do with them and includes the
trademark position. A summary here would be a second source of truth, so there
is deliberately not one.

The short version, for anyone who only reads this far: **take the software,
rebrand your fork.**

### 2. Bundled third-party work

RichOS bundles skills, tools and fonts written by other people. Each stays under
the license it arrived with — Apache-2.0, MIT and the SIL Open Font License, all
compatible with distributing the combined work under the AGPL.

`docs/legal/THIRD-PARTY-NOTICES.md` is the inventory of what is BUNDLED: sixteen
skills and tools, four font families and the authoring-time tools that never
ship. Each entry names the upstream, a pinned revision or a declared version,
the copyright holder and the path to the full terms and each was verified
against that upstream rather than carried forward.

`docs/legal/THIRD-PARTY-RUST-DEPENDENCIES.md` is the inventory of what is
COMPILED IN: every Rust package the tracked lockfiles resolve, with its
version, its declared license and whether it reaches a macOS binary. It is
generated from the lockfiles by `richos/app/scripts/dependency-license-inventory.sh`
and keyed to their sha256, so it describes a graph anybody can reproduce. Two
documents rather than one because provenance is hand-verified and a resolved
graph is derived and a file that mixes the two invites the derived half to be
trusted as far as the hand-checked half.

## Where the license text appears

| Context | Path | How it gets there |
|---|---|---|
| This repository | `LICENSE` | Committed. Canonical. Never edited. |
| The standalone engine release asset | `LICENSE`, at the top of the archive's engine directory | Copied at packaging time by `richos/app/scripts/make-engine-asset.sh`, then read back out of the built archive and compared to the canonical file before the build is accepted |

There is exactly **one** committed copy of the license text. A second committed
copy is a file that can drift from the canonical one with nothing to catch it,
which is why the engine asset's copy is produced by packaging and verified,
rather than checked in and trusted.

`richos/app/scripts/make-engine-asset.test.sh` is the gate: it builds the archive,
opens it and refuses if the license text is missing, if it is not
byte-identical to the root file, if the third-party notices are absent or if
any bundled third-party directory has lost its own license file.

## Related documents

- `LICENSE` — the license itself.
- `docs/legal/BRAND-ASSETS.md` — the brand exclusion and trademark position.
- `docs/legal/THIRD-PARTY-NOTICES.md` — bundled third-party work and its terms.
- `docs/legal/THIRD-PARTY-RUST-DEPENDENCIES.md` — the generated per-package
  inventory of every compiled Rust dependency, keyed to the lockfile digests.
- `richos/engine/LICENSING.md` — the same story scoped to the engine, which is
  distributed on its own and therefore has to be able to answer for itself.

## For anyone adding code or a dependency

New RichOS-authored files are AGPL-3.0-only by default; they need no per-file
header, because the root `LICENSE` covers the repository.

Vendoring somebody else's work means three things in the same commit: the
upstream license file next to the work, a row in
`docs/legal/THIRD-PARTY-NOTICES.md` with a pinned revision or declared version,
and — if the bundled copy differs from upstream at all — a notice in that
directory saying how.

**A dependency whose own license forbids combination with the AGPL cannot be
bundled**, however convenient it is. Proprietary binaries and non-free
redistributables are the usual trap and the cost of finding out after
publication is a re-release.
