# Voice source and delivery contract

## Source and copied layouts

The supported source layout is beneath the repository's `richos/` directory:

```text
<product-root>/
  engine/voice/
    package.json
    models/
    provisioning/
  app/crates/richos-voice/
  tools/richos-service/
  tools/richos-hud/
  tools/richos-extension/
```

Relative module/script locations determine source resolution, independent of the
working directory. A copied service needs its existing extension dependencies and
`engine/voice` at the same relative location. The native-host installer registers
the service where it lives; it does not create a standalone service distribution.
Copying only the service or only `tools/` is insufficient.

No new service distribution or engine-discovery mechanism is introduced. Existing
service entry points stay in place. The three legacy JavaScript model modules are
forwarders. The old `tools/richos-service/lib/model-*.json` files are removed;
maintained readers use the canonical files. There are no independently editable
compatibility copies or symlinks.

An engine-only extraction can run the component's core tests and provisioning
modules with Node 24.21.0 or later. It does not contain the app or the tool frontends.
Repository consumer and relocation tests explicitly require those siblings.

## App builds and installed behavior

The Rust crate remains in `app/crates/richos-voice`, with its existing package name
and workspace membership. `hardware.rs` and `toolchain.rs` include the canonical
JSON files at compile time. An app source build therefore needs `engine/voice/models`
even if no service is built. Cargo tracks these include inputs.

An installed app uses the metadata compiled into its binary. Service processes read
their matching release files. Neither resolves an arbitrary global engine pointer
for voice metadata. Replacing an engine archive alone cannot update a compiled app.
Preserve a complete matched release when upgrading or rolling back a copied service.
There is no user-data migration in this consolidation.

The existing tracked-file engine packaging includes the whole voice component,
including its ESM package boundary. The engine archive still has `engine/` as its
only top-level directory and retains the existing license/runtime inventory rules.
No speech weights or user state are added to it. Release runtime verification and
activation continue to belong to the existing app packaging system.

## Verification

`tests/relocation.test.mjs` runs core tests from an engine-only copy, then service/HUD
checks from a copied product layout containing spaces. Missing canonical files
must fail even while the original checkout remains available. The app script
`richos/app/scripts/voice-component.test.sh` runs full consumer conformance, relocation
and mutation-runner failure/interruption checks. Existing local test discovery
finds the core shell suite; voice source/data changes select it explicitly.

Stored workflow path filters account for the new build inputs. This does not enable
or dispatch remote CI. The verification receipt records the exact metadata hashes,
runtime version, packaged artifact and any installed acceptance that was not run.
