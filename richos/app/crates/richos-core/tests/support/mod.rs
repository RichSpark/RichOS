//! SHARED TEST FIXTURE: the entity registry these suites run against.
//!
//! # Why this file exists
//!
//! Until 2026-09-04 `Spine::new` arrived holding a compiled-in registry —
//! `EntityRegistry::CEOS_COMPANIES`, a `const` table of the CEO's six real companies bound
//! to absolute roots under his own home directory — and `impl Default for EntityRegistry`
//! returned it, so that table was the SHIPPING registry of every copy of RichOS ever built.
//! Every suite in `tests/` inherited it for free and said `Spine::new(ledger)`.
//!
//! The registry is per-user now (`entity.rs` rule 4): it is read from `entities.json` in the
//! install's own configuration directory and a bare `Spine` accepts NO entity until the shell
//! installs one. So the suites declare theirs, here, once — which is also what the app does
//! at boot, and therefore a more honest fixture than the one it replaces.
//!
//! # About the ids
//!
//! They are the ids the existing suites were written against, and they are kept because much
//! of the surrounding commentary is a durable record of real incidents and measurements —
//! renaming the subject of a measurement makes the record wrong. **Nothing here ships.**
//! `tests/` is not compiled into the app, the roots below exist on no machine, and the
//! product's own default registry is empty and belongs to nobody (see
//! `entity_registry_tests::the_shipping_default_registry_is_empty`).

#![allow(dead_code)]

use richos_core::entity::{Entity, EntityRegistry};
use richos_core::ledger::Ledger;
use richos_core::spine::Spine;

/// The entity areas the suites in `tests/` may file work under.
///
/// `acme-holdings` is deliberately absent: it is the negative control several suites use to
/// prove an unregistered entity can never become a thread's immutable home.
pub fn registry() -> EntityRegistry {
    EntityRegistry::new(vec![
        Entity::new("femcboost", "FemcBoost", &["/fixture/femcboost"]).unwrap(),
        Entity::new("deeply", "Deeply", &["/fixture/deeply"]).unwrap(),
    ])
    .unwrap()
}

/// A spine over `ledger` that accepts [`registry`] — the drop-in replacement for the
/// `Spine::new(ledger)` that used to arrive pre-loaded with somebody's companies.
pub fn spine(ledger: Ledger) -> Spine {
    let mut spine = Spine::new(ledger);
    spine.set_entity_registry(registry());
    spine
}

/// The factual context on the native priming wire.
pub fn context(text:&str)->serde_json::Value {
    serde_json::from_str::<serde_json::Value>(text).expect("factual priming JSON")["context"].clone()
}
