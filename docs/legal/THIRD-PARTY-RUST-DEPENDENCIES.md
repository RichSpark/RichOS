# Compiled dependency license inventory (Rust)

**Generated. Do not hand-edit.**

```
richos/app/scripts/dependency-license-inventory.sh          # regenerate
richos/app/scripts/dependency-license-inventory.sh --check  # fail if this file is stale
```

This is the per-package inventory that `docs/legal/THIRD-PARTY-NOTICES.md` makes a gate on distributing a RichOS binary. Every row comes from `cargo metadata --locked`, which refuses to run if a lockfile would have to change — so this document describes the graph the committed lockfiles pin or it does not exist.

## What it is keyed to

Identity is the lockfile, not a date. Regenerate after any dependency change and these digests move with it.

| Lockfile | Workspace | sha256 |
|---|---|---|
| `richos/app/Cargo.lock` | `richos/app` | `80d019b2f41ecb8408c468309459a86d0e4cf0a856f55b8aeeac7188f8e12934` |
| `richos/app/crates/richdoctor-core/Cargo.lock` | `richos/app/crates/richdoctor-core` | `e893282c9749897aa19bb92a25bd900327f1d212fb05e2c337a413ac0533ab03` |
| `richos/app/crates/richdoctor/Cargo.lock` | `richos/app/crates/richdoctor` | `700a1b211cca039053469768a517743ffa186f2f09d395f719497e45aed8f36c` |
| `richos/app/crates/richos-devtools/Cargo.lock` | `richos/app/crates/richos-devtools` | `6c65c04d1dd1fd3a1241bd481140e306e211ad1f05676536d292854f63b5a3ab` |
| `richos/app/crates/richos-user-update/Cargo.lock` | `richos/app/crates/richos-user-update` | `46985b0c2a0962ab8a06f36939b83c4695d46259b2245c821c7f94c6f751f1af` |
| `richos/app/src-tauri/Cargo.lock` | `richos/app/src-tauri` | `46b4edcde85fb72a73e4e1d78e5da996c7033e45b1bd10192e4822e3fc0ffb17` |
| `richos/mobile/conformance/verifier/Cargo.lock` | `richos/mobile/conformance/verifier` | `8036e74423f9ef07b0d595dca1b305e4319f85a9ba03b56ffc54c1433034f2f6` |
| `richos/tools/native-claude-stdio/Cargo.lock` | `richos/tools/native-claude-stdio` | `3f99c5fc84319ab96697e7c2086644362f6d472ebd7825425753e52e0a43380e` |

## The answer, first

**611 distinct third-party packages resolve across the 8 workspaces. Every one of them may be distributed as part of an AGPL-3.0-only combined work.** No package in this tree is proprietary and none carries terms that conflict with the AGPL.

378 of them reach a macOS binary. The rest are build-time tooling, test-only dependencies or code compiled exclusively for targets RichOS does not ship.

### The two families that are not plain attribution

**MPL-2.0 (5 packages: `cssparser`, `cssparser-macros`, `dtoa-short`, `option-ext`, `selectors`).** MPL-2.0 is reciprocal per FILE, not per program. Section 3.3 expressly permits distributing the Larger Work under a Secondary License and the GNU AGPL v3 is named as one — provided no covered file carries the Exhibit B "Incompatible With Secondary Licenses" notice. No source file in any of these packages carries it; the phrase appears only inside the license text each of them ships, where it is part of the boilerplate. The obligation RichOS carries is therefore the ordinary one: the source of those files stays available and their notices travel with the binary.

**`r-efi` (5.3.0, 6.0.0).** Offered as "MIT OR Apache-2.0 OR LGPL-2.1-or-later". The pre-publication audit flagged it twice: once because an LGPL alternative in the list must not be silently rolled into an AGPL claim and once because the two untracked lockfiles carried different versions of it.

Both are settled here and neither needed a dependency change.

*The license.* RichOS takes the MIT branch, which the offer permits outright. The package is also a UEFI binding, reached only through `getrandom`'s UEFI target support: it is absent from the resolved graph for both Darwin triples, so it is never compiled into anything RichOS ships. It appears in the lockfiles at all because a lockfile pins every platform's graph, which is the behavior that makes lockfiles worth committing.

*The versions.* 5.3.0 in `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri`; 6.0.0 in `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri`. That is not drift between a stale file and a fresh one — regenerating both lockfiles from the same index on the same day reproduces it exactly. Two things cause it. `richos/app/src-tauri` is a DELIBERATELY DETACHED workspace (the empty `[workspace]` table in its manifest, explained at the top of `richos/app/Cargo.toml`), so Cargo resolves it independently of `richos/app/` and there is no single lockfile that could cover both. And within the Tauri workspace two semver-major lines of `getrandom` coexist — `tauri` 2.11.5 pulls `getrandom` 0.3.x, which requires `r-efi` 5.x, while `tempfile` and `uuid` pull `getrandom` 0.4.x, which requires `r-efi` 6.x. Cargo keeps both because they are different major versions, which is correct behavior rather than a conflict to resolve.

It is also worth stating what carries the offer-versus-obligation distinction: most of this tree is "MIT OR Apache-2.0", which is a CHOICE. The table below records the full offer as the publisher stated it; the compatibility class records the branch RichOS relies on.

## Licenses present, by package count

| License expression | Packages | Class | What it obliges |
|---|---|---|---|
| `MIT OR Apache-2.0` | 299 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `MIT` | 115 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `Apache-2.0 OR MIT` | 43 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `MIT/Apache-2.0` | 30 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `Zlib OR Apache-2.0 OR MIT` | 23 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `Unicode-3.0` | 18 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `Apache-2.0` | 14 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `Unlicense OR MIT` | 9 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | 6 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `ISC` | 6 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `Apache-2.0/MIT` | 5 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `MPL-2.0` | 5 | file-copyleft | Per-file reciprocal. MPL-2.0 section 3.3 permits distributing the Larger Work under the GNU AGPL v3 and the source of the covered files must stay available. |
| `Apache-2.0 OR ISC OR MIT` | 3 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `BSD-3-Clause` | 3 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `Zlib` | 3 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | 2 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `Apache-2.0 OR MIT OR Zlib` | 2 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `BSD-2-Clause OR Apache-2.0 OR MIT` | 2 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `BSD-3-Clause OR MIT OR Apache-2.0` | 2 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | 2 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `MIT OR Apache-2.0 OR Zlib` | 2 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `MIT OR Zlib OR Apache-2.0` | 2 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `Unlicense/MIT` | 2 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `0BSD OR MIT OR Apache-2.0` | 1 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `Apache-2.0 / MIT` | 1 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `Apache-2.0 AND ISC` | 1 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `Apache-2.0 AND MIT` | 1 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `Apache-2.0 OR BSL-1.0` | 1 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `Apache-2.0 WITH LLVM-exception` | 1 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `BSD-2-Clause` | 1 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `BSD-2-Clause OR MIT OR Apache-2.0` | 1 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `BSD-3-Clause AND MIT` | 1 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `BSD-3-Clause/MIT` | 1 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `CC0-1.0` | 1 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `CC0-1.0 OR MIT-0 OR Apache-2.0` | 1 | permissive | Attribution only. Combines with AGPL-3.0-only without further obligation. |
| `CDLA-Permissive-2.0` | 1 | data | A permissive license over data rather than code. No reciprocal obligation. |

## Every package

"Reaches macOS binary" is derived from the resolved graph filtered to `aarch64-apple-darwin` and `x86_64-apple-darwin`, following normal dependency edges from the workspace members. "build only" means it is reached only through a build-dependency edge; "no" means it is reached only by dev/test edges or is gated to a target RichOS does not build for.

| Package | Version | License | Reaches macOS binary | Workspace |
|---|---|---|---|---|
| `adler2` | 2.0.1 | `0BSD OR MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `ahash` | 0.8.12 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `aho-corasick` | 1.1.5 | `Unlicense OR MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richos-devtools`, `richos/app/src-tauri` |
| `alloc-no-stdlib` | 2.0.4 | `BSD-3-Clause` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `alloc-stdlib` | 0.2.4 | `BSD-3-Clause` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `alsa` | 0.11.0 | `Apache-2.0/MIT` | no (other targets or tests only) | `richos/app`, `richos/app/src-tauri` |
| `alsa-sys` | 0.4.0 | `MIT` | no (other targets or tests only) | `richos/app`, `richos/app/src-tauri` |
| `android_system_properties` | 0.1.6 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `anyhow` | 1.0.104 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `arbitrary` | 1.4.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/src-tauri` |
| `atk` | 0.18.2 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `atk-sys` | 0.18.2 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `atomic-waker` | 1.1.2 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `autocfg` | 1.5.1 | `Apache-2.0 OR MIT` | build only | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `base64` | 0.13.1 | `MIT/Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `base64` | 0.21.7 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `base64` | 0.22.1 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier` |
| `base64` | 0.23.1 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `base64ct` | 1.8.3 | `Apache-2.0 OR MIT` | yes | `richos/app`, `richos/app/src-tauri` |
| `beef` | 0.5.2 | `MIT OR Apache-2.0` | build only | `richos/app`, `richos/app/src-tauri` |
| `bit-set` | 0.8.0 | `Apache-2.0 OR MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `bit-vec` | 0.8.0 | `Apache-2.0 OR MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `bitflags` | 1.3.2 | `MIT/Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `bitflags` | 2.13.1 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `block-buffer` | 0.10.4 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `block2` | 0.6.2 | `MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `brotli` | 8.0.4 | `BSD-3-Clause AND MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `brotli-decompressor` | 5.0.3 | `BSD-3-Clause/MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `bs58` | 0.5.1 | `MIT/Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `bumpalo` | 3.20.3 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `bytemuck` | 1.25.2 | `Zlib OR Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `byteorder` | 1.5.0 | `Unlicense OR MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `bytes` | 1.12.1 | `MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `cairo-rs` | 0.18.5 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `cairo-sys-rs` | 0.18.2 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `camino` | 1.2.5 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `cargo-platform` | 0.1.9 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `cargo_metadata` | 0.19.2 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `cargo_toml` | 0.22.3 | `Apache-2.0 OR MIT` | build only | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `castaway` | 0.2.4 | `MIT` | yes | `richos/app`, `richos/app/src-tauri` |
| `cc` | 1.4.4 | `MIT OR Apache-2.0` | build only | `richos/app/crates/richdoctor`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier` |
| `cc` | 1.8.0 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app` |
| `cesu8` | 1.1.0 | `Apache-2.0/MIT` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `cfb` | 0.7.3 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `cfg-expr` | 0.15.8 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `cfg-if` | 1.0.4 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier` |
| `cfg-if` | 1.0.5 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richos-devtools` |
| `chrono` | 0.4.45 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `combine` | 4.6.8 | `MIT` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `compact_str` | 0.9.1 | `MIT` | yes | `richos/app`, `richos/app/src-tauri` |
| `cookie` | 0.18.2 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `core-foundation` | 0.10.1 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `core-foundation` | 0.9.4 | `MIT OR Apache-2.0` | yes | `richos/app/src-tauri` |
| `core-foundation-sys` | 0.8.7 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `core-graphics` | 0.25.0 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `core-graphics-types` | 0.2.0 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `coreaudio-rs` | 0.14.2 | `MIT/Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `cpal` | 0.17.3 | `Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `cpufeatures` | 0.2.17 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `crc32fast` | 1.5.1 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `crc32fast` | 1.5.2 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor-core` |
| `crossbeam-channel` | 0.5.16 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `crossbeam-deque` | 0.8.8 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `crossbeam-epoch` | 0.9.21 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `crossbeam-utils` | 0.8.22 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `crossbeam-utils` | 0.8.23 | `MIT OR Apache-2.0` | yes | `richos/app` |
| `crypto-common` | 0.1.7 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `cssparser` | 0.36.0 | `MPL-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `cssparser-macros` | 0.6.1 | `MPL-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `ctor` | 0.8.0 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `ctor-proc-macro` | 0.0.7 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `darling` | 0.20.11 | `MIT` | yes | `richos/app`, `richos/app/src-tauri` |
| `darling` | 0.23.0 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `darling_core` | 0.20.11 | `MIT` | yes | `richos/app`, `richos/app/src-tauri` |
| `darling_core` | 0.23.0 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `darling_macro` | 0.20.11 | `MIT` | yes | `richos/app`, `richos/app/src-tauri` |
| `darling_macro` | 0.23.0 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `dary_heap` | 0.3.9 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `dasp_sample` | 0.11.0 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `dbus` | 0.9.12 | `Apache-2.0/MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `defmt` | 1.1.1 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `defmt-macros` | 1.1.1 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `defmt-parser` | 1.0.0 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `der` | 0.8.2 | `Apache-2.0 OR MIT` | yes | `richos/app`, `richos/app/src-tauri` |
| `deranged` | 0.5.8 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `derive_arbitrary` | 1.4.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/src-tauri` |
| `derive_builder` | 0.20.2 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `derive_builder_core` | 0.20.2 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `derive_builder_macro` | 0.20.2 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `derive_more` | 2.1.1 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `derive_more-impl` | 2.1.1 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `digest` | 0.10.7 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `dirs` | 6.0.0 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `dirs-sys` | 0.5.0 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `dispatch2` | 0.3.1 | `Zlib OR Apache-2.0 OR MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `displaydoc` | 0.2.7 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `dlopen2` | 0.8.2 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `dlopen2_derive` | 0.4.3 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `dom_query` | 0.27.0 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `dpi` | 0.1.2 | `Apache-2.0 AND MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `dtoa` | 1.0.11 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `dtoa-short` | 0.3.5 | `MPL-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `dtor` | 0.3.0 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `dtor-proc-macro` | 0.0.6 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `dunce` | 1.0.5 | `CC0-1.0 OR MIT-0 OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `dyn-clone` | 1.0.20 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `either` | 1.19.0 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `embed-resource` | 3.0.11 | `MIT` | build only | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `embed_plist` | 1.2.2 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `equivalent` | 1.0.2 | `Apache-2.0 OR MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `erased-serde` | 0.4.10 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `errno` | 0.3.14 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `esaxx-rs` | 0.1.10 | `Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `fancy-regex` | 0.14.0 | `MIT` | yes | `richos/app`, `richos/app/src-tauri` |
| `fastrand` | 2.5.0 | `Apache-2.0 OR MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `fdeflate` | 0.3.7 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `field-offset` | 0.3.6 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `filetime` | 0.2.29 | `MIT/Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `find-msvc-tools` | 0.1.11 | `MIT OR Apache-2.0` | build only | `richos/app/crates/richdoctor`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier` |
| `find-msvc-tools` | 0.1.14 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app` |
| `fixedbitset` | 0.5.7 | `MIT OR Apache-2.0` | build only | `richos/app`, `richos/app/src-tauri` |
| `flate2` | 1.1.10 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `fnv` | 1.0.7 | `Apache-2.0 / MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `foldhash` | 0.2.0 | `Zlib` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `foreign-types` | 0.3.2 | `MIT/Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/src-tauri` |
| `foreign-types` | 0.5.0 | `MIT/Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `foreign-types-macros` | 0.2.4 | `MIT/Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `foreign-types-shared` | 0.1.1 | `MIT/Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/src-tauri` |
| `foreign-types-shared` | 0.3.1 | `MIT/Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `form_urlencoded` | 1.2.2 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `fsevent-sys` | 4.1.0 | `MIT` | yes | `richos/app/src-tauri` |
| `futures-channel` | 0.3.34 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `futures-core` | 0.3.34 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `futures-executor` | 0.3.34 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `futures-io` | 0.3.34 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `futures-macro` | 0.3.34 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `futures-sink` | 0.3.34 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `futures-task` | 0.3.34 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `futures-util` | 0.3.34 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `gdk` | 0.18.2 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `gdk-pixbuf` | 0.18.5 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `gdk-pixbuf-sys` | 0.18.0 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `gdk-sys` | 0.18.2 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `gdkwayland-sys` | 0.18.2 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `gdkx11` | 0.18.2 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `gdkx11-sys` | 0.18.2 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `generic-array` | 0.14.7 | `MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `getrandom` | 0.2.17 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier` |
| `getrandom` | 0.3.4 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `getrandom` | 0.4.3 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `gio` | 0.18.4 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `gio-sys` | 0.18.1 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `glib` | 0.18.5 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `glib-macros` | 0.18.5 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `glib-sys` | 0.18.1 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `glob` | 0.3.4 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `gobject-sys` | 0.18.0 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `gtk` | 0.18.2 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `gtk-sys` | 0.18.2 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `gtk3-macros` | 0.18.2 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `hashbrown` | 0.12.3 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `hashbrown` | 0.17.1 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `heck` | 0.4.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `heck` | 0.5.0 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `hex` | 0.4.3 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `hmac-sha256` | 1.1.15 | `ISC` | build only | `richos/app`, `richos/app/src-tauri` |
| `html5ever` | 0.38.0 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `http` | 1.5.0 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `http-body` | 1.1.0 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `http-body-util` | 0.1.5 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `httparse` | 1.10.1 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `httpdate` | 1.0.3 | `MIT OR Apache-2.0` | yes | `richos/app/src-tauri` |
| `hyper` | 1.11.1 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `hyper-rustls` | 0.27.9 | `Apache-2.0 OR ISC OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `hyper-util` | 0.1.20 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `iana-time-zone` | 0.1.65 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `iana-time-zone-haiku` | 0.1.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `ico` | 0.5.0 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `icu_collections` | 2.3.0 | `Unicode-3.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `icu_locale_core` | 2.3.0 | `Unicode-3.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `icu_normalizer` | 2.3.0 | `Unicode-3.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `icu_normalizer_data` | 2.3.0 | `Unicode-3.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `icu_properties` | 2.3.0 | `Unicode-3.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `icu_properties_data` | 2.3.0 | `Unicode-3.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `icu_provider` | 2.3.1 | `Unicode-3.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `ident_case` | 1.0.1 | `MIT/Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `idna` | 1.1.0 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `idna_adapter` | 1.2.2 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `indexmap` | 1.9.3 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `indexmap` | 2.14.1 | `Apache-2.0 OR MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `infer` | 0.19.0 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `inotify` | 0.11.5 | `ISC` | no (other targets or tests only) | `richos/app/src-tauri` |
| `inotify-sys` | 0.1.8 | `ISC` | no (other targets or tests only) | `richos/app/src-tauri` |
| `ipnet` | 2.12.1 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `itertools` | 0.14.0 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `itoa` | 1.0.18 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier`, `richos/tools/native-claude-stdio` |
| `javascriptcore-rs` | 1.1.2 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `javascriptcore-rs-sys` | 1.1.1 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `jiff` | 0.2.35 | `Unlicense OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `jiff-core` | 0.1.0 | `Unlicense OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `jiff-static` | 0.2.35 | `Unlicense OR MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `jiff-tzdb` | 0.1.8 | `Unlicense OR MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `jiff-tzdb-platform` | 0.1.3 | `Unlicense OR MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `jni` | 0.21.1 | `MIT/Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `jni` | 0.22.4 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `jni-macros` | 0.22.4 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `jni-sys` | 0.3.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `jni-sys` | 0.4.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `jni-sys-macros` | 0.4.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `js-sys` | 0.3.104 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `json-patch` | 3.0.1 | `MIT/Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `jsonptr` | 0.6.3 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `keyboard-types` | 0.7.0 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `kqueue` | 1.2.1 | `MIT` | no (other targets or tests only) | `richos/app/src-tauri` |
| `kqueue-sys` | 1.1.2 | `MIT` | no (other targets or tests only) | `richos/app/src-tauri` |
| `lazy_static` | 1.5.1 | `MIT OR Apache-2.0` | build only | `richos/app`, `richos/app/src-tauri` |
| `libappindicator` | 0.9.0 | `Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `libappindicator-sys` | 0.9.0 | `Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `libc` | 0.2.189 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier` |
| `libc` | 0.2.190 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richos-devtools` |
| `libdbus-sys` | 0.2.7 | `Apache-2.0/MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `libloading` | 0.7.4 | `ISC` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `libredox` | 0.1.23 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `linux-raw-sys` | 0.12.1 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `litemap` | 0.8.3 | `Unicode-3.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `lock_api` | 0.4.14 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `log` | 0.4.34 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `logos` | 0.14.4 | `MIT OR Apache-2.0` | build only | `richos/app`, `richos/app/src-tauri` |
| `logos-codegen` | 0.14.4 | `MIT OR Apache-2.0` | build only | `richos/app`, `richos/app/src-tauri` |
| `logos-derive` | 0.14.4 | `MIT OR Apache-2.0` | build only | `richos/app`, `richos/app/src-tauri` |
| `lzma-rust2` | 0.15.8 | `Apache-2.0` | build only | `richos/app`, `richos/app/src-tauri` |
| `mach2` | 0.5.0 | `BSD-2-Clause OR MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `macro_rules_attribute` | 0.2.3 | `Apache-2.0 OR MIT OR Zlib` | yes | `richos/app`, `richos/app/src-tauri` |
| `macro_rules_attribute-proc_macro` | 0.2.3 | `Apache-2.0 OR MIT OR Zlib` | yes | `richos/app`, `richos/app/src-tauri` |
| `markup5ever` | 0.38.0 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `matrixmultiply` | 0.3.11 | `MIT/Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `memchr` | 2.8.3 | `Unlicense OR MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier`, `richos/tools/native-claude-stdio` |
| `memoffset` | 0.9.1 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `miette` | 7.6.0 | `Apache-2.0` | build only | `richos/app`, `richos/app/src-tauri` |
| `miette-derive` | 7.6.0 | `Apache-2.0` | build only | `richos/app`, `richos/app/src-tauri` |
| `mime` | 0.3.17 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `minimal-lexical` | 0.2.1 | `MIT/Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `minisign-verify` | 0.2.5 | `MIT` | yes | `richos/app/src-tauri` |
| `miniz_oxide` | 0.8.9 | `MIT OR Zlib OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `miniz_oxide` | 0.9.1 | `MIT OR Zlib OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `mio` | 1.2.3 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `monostate` | 0.1.18 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `monostate-impl` | 0.1.18 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `muda` | 0.19.3 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `multimap` | 0.10.1 | `MIT OR Apache-2.0` | build only | `richos/app`, `richos/app/src-tauri` |
| `native-tls` | 0.2.18 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `ndarray` | 0.17.2 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `ndk` | 0.9.0 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `ndk-context` | 0.1.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/src-tauri` |
| `ndk-sys` | 0.6.0+11769913 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `new_debug_unreachable` | 1.0.6 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `nom` | 7.1.3 | `MIT` | yes | `richos/app`, `richos/app/src-tauri` |
| `notify` | 8.2.0 | `CC0-1.0` | yes | `richos/app/src-tauri` |
| `notify-types` | 2.1.0 | `MIT OR Apache-2.0` | yes | `richos/app/src-tauri` |
| `num-complex` | 0.4.6 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `num-conv` | 0.2.2 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `num-derive` | 0.4.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/src-tauri` |
| `num-integer` | 0.1.47 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `num-traits` | 0.2.19 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `num_enum` | 0.7.6 | `BSD-3-Clause OR MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `num_enum_derive` | 0.7.6 | `BSD-3-Clause OR MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `objc2` | 0.6.4 | `MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `objc2-app-kit` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `objc2-audio-toolbox` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | yes | `richos/app`, `richos/app/src-tauri` |
| `objc2-avf-audio` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app`, `richos/app/src-tauri` |
| `objc2-cloud-kit` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `objc2-core-audio` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | yes | `richos/app`, `richos/app/src-tauri` |
| `objc2-core-audio-types` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | yes | `richos/app`, `richos/app/src-tauri` |
| `objc2-core-data` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `objc2-core-foundation` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `objc2-core-graphics` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `objc2-core-image` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `objc2-core-location` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `objc2-core-text` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `objc2-encode` | 4.1.0 | `MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `objc2-exception-helper` | 0.1.1 | `Zlib OR Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `objc2-foundation` | 0.3.2 | `MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `objc2-io-surface` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `objc2-osa-kit` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | yes | `richos/app/src-tauri` |
| `objc2-quartz-core` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `objc2-ui-kit` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `objc2-user-notifications` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `objc2-web-kit` | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `once_cell` | 1.21.4 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `openssl` | 0.10.81 | `Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/src-tauri` |
| `openssl-macros` | 0.1.1 | `MIT/Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/src-tauri` |
| `openssl-probe` | 0.2.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `openssl-sys` | 0.9.117 | `MIT` | no (other targets or tests only) | `richos/app`, `richos/app/src-tauri` |
| `option-ext` | 0.2.0 | `MPL-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `ort` | 2.0.0-rc.12 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `ort-sys` | 2.0.0-rc.12 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `osakit` | 0.3.1 | `MIT OR Apache-2.0` | yes | `richos/app/src-tauri` |
| `pango` | 0.18.3 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `pango-sys` | 0.18.0 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `parking_lot` | 0.12.5 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `parking_lot_core` | 0.9.12 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `paste` | 1.0.15 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `pastey` | 0.2.3 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `pem-rfc7468` | 1.0.0 | `Apache-2.0 OR MIT` | yes | `richos/app`, `richos/app/src-tauri` |
| `percent-encoding` | 2.3.2 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `petgraph` | 0.7.1 | `MIT OR Apache-2.0` | build only | `richos/app`, `richos/app/src-tauri` |
| `phf` | 0.13.1 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `phf_codegen` | 0.13.1 | `MIT` | build only | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `phf_generator` | 0.13.1 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `phf_macros` | 0.13.1 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `phf_shared` | 0.13.1 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `pin-project-lite` | 0.2.17 | `Apache-2.0 OR MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `pkg-config` | 0.3.34 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `plist` | 1.10.0 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `png` | 0.17.16 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `png` | 0.18.1 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `portable-atomic` | 1.15.0 | `Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `portable-atomic-util` | 0.2.7 | `Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `portable-atomic-util` | 0.2.8 | `Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app` |
| `potential_utf` | 0.1.6 | `Unicode-3.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `powerfmt` | 0.2.0 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `ppv-lite86` | 0.2.21 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `precomputed-hash` | 0.1.1 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `prettyplease` | 0.2.37 | `MIT OR Apache-2.0` | build only | `richos/app`, `richos/app/src-tauri` |
| `proc-macro-crate` | 1.3.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `proc-macro-crate` | 2.0.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `proc-macro-crate` | 3.5.0 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `proc-macro-error` | 1.0.4 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `proc-macro-error-attr` | 1.0.4 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `proc-macro2` | 1.0.107 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier`, `richos/tools/native-claude-stdio` |
| `prost` | 0.13.5 | `Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `prost-build` | 0.13.5 | `Apache-2.0` | build only | `richos/app`, `richos/app/src-tauri` |
| `prost-derive` | 0.13.5 | `Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `prost-reflect` | 0.14.7 | `MIT OR Apache-2.0` | build only | `richos/app`, `richos/app/src-tauri` |
| `prost-types` | 0.13.5 | `Apache-2.0` | build only | `richos/app`, `richos/app/src-tauri` |
| `protox` | 0.7.2 | `MIT OR Apache-2.0` | build only | `richos/app`, `richos/app/src-tauri` |
| `protox-parse` | 0.7.0 | `MIT OR Apache-2.0` | build only | `richos/app`, `richos/app/src-tauri` |
| `quick-xml` | 0.41.0 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `quote` | 1.0.47 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier`, `richos/tools/native-claude-stdio` |
| `r-efi` | 5.3.0 | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `r-efi` | 6.0.0 | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `rand` | 0.9.5 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `rand_chacha` | 0.9.0 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `rand_core` | 0.9.5 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `raw-window-handle` | 0.6.2 | `MIT OR Apache-2.0 OR Zlib` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `rawpointer` | 0.2.1 | `MIT/Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `rayon` | 1.12.0 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `rayon-cond` | 0.4.0 | `Apache-2.0/MIT` | yes | `richos/app`, `richos/app/src-tauri` |
| `rayon-core` | 1.13.0 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `redox_syscall` | 0.5.18 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `redox_users` | 0.5.2 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `ref-cast` | 1.0.27 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `ref-cast-impl` | 1.0.27 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `regex` | 1.13.1 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richos-devtools`, `richos/app/src-tauri` |
| `regex-automata` | 0.4.18 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richos-devtools`, `richos/app/src-tauri` |
| `regex-syntax` | 0.8.11 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richos-devtools`, `richos/app/src-tauri` |
| `reqwest` | 0.13.4 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `rfd` | 0.16.0 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `ring` | 0.17.14 | `Apache-2.0 AND ISC` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier` |
| `rustc-hash` | 2.1.3 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `rustc_version` | 0.4.1 | `MIT OR Apache-2.0` | build only | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `rustix` | 1.1.4 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `rustix` | 1.1.5 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richos-devtools` |
| `rustls` | 0.23.45 | `Apache-2.0 OR ISC OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `rustls-native-certs` | 0.8.4 | `Apache-2.0 OR ISC OR MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `rustls-pki-types` | 1.15.1 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `rustls-platform-verifier` | 0.7.0 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `rustls-platform-verifier-android` | 0.1.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `rustls-webpki` | 0.103.15 | `ISC` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `rustversion` | 1.0.23 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `ryu` | 1.0.23 | `Apache-2.0 OR BSL-1.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `same-file` | 1.0.6 | `Unlicense/MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `schannel` | 0.1.29 | `MIT` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `schemars` | 0.8.22 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `schemars` | 0.9.0 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `schemars` | 1.2.2 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `schemars_derive` | 0.8.22 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `scopeguard` | 1.2.0 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `security-framework` | 3.7.0 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `security-framework-sys` | 2.17.0 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `selectors` | 0.36.1 | `MPL-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `semver` | 1.0.28 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `sentencepiece-model` | 0.1.4 | `BSD-2-Clause` | yes | `richos/app`, `richos/app/src-tauri` |
| `serde` | 1.0.229 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier`, `richos/tools/native-claude-stdio` |
| `serde-untagged` | 0.1.9 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `serde_core` | 1.0.229 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier`, `richos/tools/native-claude-stdio` |
| `serde_derive` | 1.0.229 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier`, `richos/tools/native-claude-stdio` |
| `serde_derive_internals` | 0.29.1 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `serde_json` | 1.0.151 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier`, `richos/tools/native-claude-stdio` |
| `serde_repr` | 0.1.21 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `serde_spanned` | 0.6.9 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `serde_spanned` | 1.1.1 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `serde_with` | 3.22.0 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `serde_with_macros` | 3.22.0 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `serialize-to-javascript` | 0.1.2 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `serialize-to-javascript-impl` | 0.1.2 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `servo_arc` | 0.4.3 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `sha2` | 0.10.9 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `shlex` | 2.0.1 | `MIT OR Apache-2.0` | build only | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier` |
| `simd-adler32` | 0.3.10 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `simd_cesu8` | 1.2.0 | `Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `simdutf8` | 0.1.5 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `siphasher` | 1.0.3 | `MIT/Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `slab` | 0.4.12 | `MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `smallvec` | 1.16.0 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `smallvec` | 1.16.3 | `MIT OR Apache-2.0` | yes | `richos/app` |
| `socket2` | 0.6.5 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `socks` | 0.3.4 | `MIT/Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `softbuffer` | 0.4.8 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `soup3` | 0.5.0 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `soup3-sys` | 0.5.0 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `spm_precompiled` | 0.1.4 | `Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `stable_deref_trait` | 1.2.1 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `static_assertions` | 1.1.0 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `string_cache` | 0.9.0 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `string_cache_codegen` | 0.6.1 | `MIT OR Apache-2.0` | build only | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `strsim` | 0.11.1 | `MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `subtle` | 2.6.1 | `BSD-3-Clause` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `swift-rs` | 1.0.8 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `syn` | 1.0.109 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `syn` | 2.0.119 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `syn` | 3.0.4 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri`, `richos/tools/native-claude-stdio` |
| `syn` | 3.0.5 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richos-user-update`, `richos/mobile/conformance/verifier` |
| `syn` | 3.0.9 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richos-devtools` |
| `sync_wrapper` | 1.0.2 | `Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `synstructure` | 0.13.2 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `system-configuration` | 0.7.0 | `MIT OR Apache-2.0` | yes | `richos/app/src-tauri` |
| `system-configuration-sys` | 0.6.0 | `MIT OR Apache-2.0` | yes | `richos/app/src-tauri` |
| `system-deps` | 6.2.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tao` | 0.35.3 | `Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tao-macros` | 0.1.4 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tar` | 0.4.46 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `target-lexicon` | 0.12.16 | `Apache-2.0 WITH LLVM-exception` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tauri` | 2.11.5 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tauri-build` | 2.6.3 | `Apache-2.0 OR MIT` | build only | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tauri-codegen` | 2.6.3 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tauri-macros` | 2.6.3 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tauri-plugin` | 2.6.3 | `Apache-2.0 OR MIT` | build only | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tauri-plugin-dialog` | 2.7.1 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tauri-plugin-fs` | 2.5.1 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tauri-plugin-updater` | 2.11.0 | `Apache-2.0 OR MIT` | yes | `richos/app/src-tauri` |
| `tauri-runtime` | 2.11.3 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tauri-runtime-wry` | 2.11.4 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tauri-utils` | 2.9.3 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tauri-winres` | 0.3.6 | `MIT` | build only | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tempfile` | 3.27.0 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `tendril` | 0.5.1 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `thiserror` | 1.0.69 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `thiserror` | 2.0.20 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `thiserror` | 2.0.21 | `MIT OR Apache-2.0` | yes | `richos/app` |
| `thiserror-impl` | 1.0.69 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `thiserror-impl` | 2.0.20 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `thiserror-impl` | 2.0.21 | `MIT OR Apache-2.0` | yes | `richos/app` |
| `time` | 0.3.55 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `time-core` | 0.1.9 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `time-macros` | 0.2.32 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `tinystr` | 0.8.4 | `Unicode-3.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tinyvec` | 1.13.2 | `Zlib OR Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `tinyvec` | 1.13.3 | `Zlib OR Apache-2.0 OR MIT` | yes | `richos/app` |
| `tinyvec_macros` | 0.1.1 | `MIT OR Apache-2.0 OR Zlib` | yes | `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `tokenizers` | 0.22.2 | `Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `tokio` | 1.53.1 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tokio-rustls` | 0.26.4 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tokio-util` | 0.7.19 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `toml` | 0.8.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `toml` | 0.9.12+spec-1.1.0 | `MIT OR Apache-2.0` | build only | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `toml` | 1.1.5+spec-1.1.0 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `toml_datetime` | 0.6.3 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `toml_datetime` | 0.7.5+spec-1.1.0 | `MIT OR Apache-2.0` | build only | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `toml_datetime` | 1.1.1+spec-1.1.0 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `toml_edit` | 0.19.15 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `toml_edit` | 0.20.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `toml_edit` | 0.25.13+spec-1.1.0 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `toml_parser` | 1.1.3+spec-1.1.0 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `toml_writer` | 1.1.2+spec-1.1.0 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tower` | 0.5.3 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tower-http` | 0.6.11 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tower-layer` | 0.3.3 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tower-service` | 0.3.3 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tracing` | 0.1.44 | `MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tracing-core` | 0.1.36 | `MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `tray-icon` | 0.24.2 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `try-lock` | 0.2.5 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `typeid` | 1.0.3 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `typenum` | 1.20.1 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `unic-char-property` | 0.9.0 | `MIT/Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `unic-char-range` | 0.9.0 | `MIT/Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `unic-common` | 0.9.0 | `MIT/Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `unic-ucd-ident` | 0.9.0 | `MIT/Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `unic-ucd-version` | 0.9.0 | `MIT/Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `unicode-ident` | 1.0.24 | `(MIT OR Apache-2.0) AND Unicode-3.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier`, `richos/tools/native-claude-stdio` |
| `unicode-ident` | 1.0.27 | `(MIT OR Apache-2.0) AND Unicode-3.0` | yes | `richos/app/crates/richos-devtools` |
| `unicode-normalization` | 0.1.25 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `unicode-normalization-alignments` | 0.1.12 | `MIT/Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `unicode-segmentation` | 1.13.3 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `unicode-width` | 0.1.14 | `MIT OR Apache-2.0` | build only | `richos/app`, `richos/app/src-tauri` |
| `unicode_categories` | 0.1.1 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `untrusted` | 0.9.0 | `ISC` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier` |
| `ureq` | 3.4.2 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `ureq-proto` | 0.6.4 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `url` | 2.5.8 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `urlpattern` | 0.3.0 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `utf8-zero` | 0.8.1 | `MIT OR Apache-2.0` | yes | `richos/app`, `richos/app/src-tauri` |
| `utf8_iter` | 1.0.4 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `uuid` | 1.26.0 | `Apache-2.0 OR MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `vcpkg` | 0.2.15 | `MIT/Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/src-tauri` |
| `version-compare` | 0.2.1 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `version_check` | 0.9.5 | `MIT/Apache-2.0` | build only | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `vswhom` | 0.1.0 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `vswhom-sys` | 0.1.3 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `walkdir` | 2.5.0 | `Unlicense/MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `want` | 0.3.1 | `MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `wasi` | 0.11.1+wasi-snapshot-preview1 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier` |
| `wasip2` | 1.0.4+wasi-0.2.12 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `wasm-bindgen` | 0.2.127 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `wasm-bindgen-futures` | 0.4.77 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `wasm-bindgen-macro` | 0.2.127 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `wasm-bindgen-macro-support` | 0.2.127 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `wasm-bindgen-shared` | 0.2.127 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/src-tauri` |
| `wasm-streams` | 0.5.0 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `web-sys` | 0.3.104 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `web_atoms` | 0.2.6 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `webkit2gtk` | 2.0.2 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `webkit2gtk-sys` | 2.0.2 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `webpki-root-certs` | 1.0.9 | `CDLA-Permissive-2.0` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `webview2-com` | 0.38.2 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `webview2-com-macros` | 0.8.1 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `webview2-com-sys` | 0.38.2 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `winapi` | 0.3.9 | `MIT/Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `winapi-i686-pc-windows-gnu` | 0.4.0 | `MIT/Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `winapi-util` | 0.1.11 | `Unlicense OR MIT` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `winapi-x86_64-pc-windows-gnu` | 0.4.0 | `MIT/Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `window-vibrancy` | 0.6.0 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows` | 0.61.3 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows` | 0.62.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/src-tauri` |
| `windows-collections` | 0.2.0 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows-collections` | 0.3.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/src-tauri` |
| `windows-core` | 0.61.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows-core` | 0.62.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows-future` | 0.2.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows-future` | 0.3.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/src-tauri` |
| `windows-implement` | 0.60.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows-interface` | 0.59.3 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows-link` | 0.1.3 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows-link` | 0.2.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `windows-numerics` | 0.2.0 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows-numerics` | 0.3.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/src-tauri` |
| `windows-registry` | 0.6.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/src-tauri` |
| `windows-result` | 0.3.4 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows-result` | 0.4.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows-strings` | 0.4.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows-strings` | 0.5.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows-sys` | 0.45.0 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows-sys` | 0.52.0 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier` |
| `windows-sys` | 0.59.0 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows-sys` | 0.60.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows-sys` | 0.61.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `windows-targets` | 0.42.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows-targets` | 0.52.6 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier` |
| `windows-targets` | 0.53.5 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows-threading` | 0.1.0 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows-threading` | 0.2.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/src-tauri` |
| `windows-version` | 0.1.7 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows_aarch64_gnullvm` | 0.42.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows_aarch64_gnullvm` | 0.52.6 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier` |
| `windows_aarch64_gnullvm` | 0.53.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows_aarch64_msvc` | 0.42.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows_aarch64_msvc` | 0.52.6 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier` |
| `windows_aarch64_msvc` | 0.53.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows_i686_gnu` | 0.42.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows_i686_gnu` | 0.52.6 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier` |
| `windows_i686_gnu` | 0.53.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows_i686_gnullvm` | 0.52.6 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier` |
| `windows_i686_gnullvm` | 0.53.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows_i686_msvc` | 0.42.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows_i686_msvc` | 0.52.6 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier` |
| `windows_i686_msvc` | 0.53.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows_x86_64_gnu` | 0.42.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows_x86_64_gnu` | 0.52.6 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier` |
| `windows_x86_64_gnu` | 0.53.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows_x86_64_gnullvm` | 0.42.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows_x86_64_gnullvm` | 0.52.6 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier` |
| `windows_x86_64_gnullvm` | 0.53.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows_x86_64_msvc` | 0.42.2 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `windows_x86_64_msvc` | 0.52.6 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier` |
| `windows_x86_64_msvc` | 0.53.1 | `MIT OR Apache-2.0` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `winnow` | 0.5.40 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `winnow` | 0.7.15 | `MIT` | build only | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `winnow` | 1.0.4 | `MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `winreg` | 0.55.0 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `wit-bindgen` | 0.57.1 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `writeable` | 0.6.4 | `Unicode-3.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `wry` | 0.55.1 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `x11` | 2.21.0 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `x11-dl` | 2.21.0 | `MIT` | no (other targets or tests only) | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `xattr` | 1.6.1 | `MIT OR Apache-2.0` | yes | `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `yoke` | 0.8.3 | `Unicode-3.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `yoke-derive` | 0.8.2 | `Unicode-3.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `zerocopy` | 0.8.63 | `BSD-2-Clause OR Apache-2.0 OR MIT` | yes | `richos/app`, `richos/app/src-tauri` |
| `zerocopy-derive` | 0.8.63 | `BSD-2-Clause OR Apache-2.0 OR MIT` | no (other targets or tests only) | `richos/app`, `richos/app/src-tauri` |
| `zerofrom` | 0.1.8 | `Unicode-3.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `zerofrom-derive` | 0.1.7 | `Unicode-3.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `zeroize` | 1.9.0 | `Apache-2.0 OR MIT` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `zeroize` | 1.9.1 | `Apache-2.0 OR MIT` | yes | `richos/app` |
| `zerotrie` | 0.2.5 | `Unicode-3.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `zerovec` | 0.11.8 | `Unicode-3.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `zerovec-derive` | 0.11.6 | `Unicode-3.0` | yes | `richos/app/crates/richdoctor`, `richos/app/src-tauri` |
| `zip` | 4.6.1 | `MIT` | no (other targets or tests only) | `richos/app/src-tauri` |
| `zlib-rs` | 0.6.7 | `Zlib` | yes | `richos/app/crates/richdoctor`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri` |
| `zlib-rs` | 0.6.8 | `Zlib` | yes | `richos/app/crates/richdoctor-core` |
| `zmij` | 1.0.23 | `MIT` | yes | `richos/app`, `richos/app/crates/richdoctor`, `richos/app/crates/richdoctor-core`, `richos/app/crates/richos-devtools`, `richos/app/crates/richos-user-update`, `richos/app/src-tauri`, `richos/mobile/conformance/verifier`, `richos/tools/native-claude-stdio` |

## What this file does not cover

- **Bundled skills, fonts and tools.** They are not compiled dependencies and a resolver knows nothing about their provenance. They are hand-verified against upstream in `docs/legal/THIRD-PARTY-NOTICES.md`.
- **Whether a declared license is true.** Each row reproduces the publisher's own claim from the manifest cargo resolved to.
- **The Node dependencies of the browser test harness.** They are devDependencies of `richos/app/ui/tests` and are excluded from the shipped frontend by `richos/app/src-tauri/build.rs`, which `richos/app/scripts/frontend-payload.test.sh` gates.
