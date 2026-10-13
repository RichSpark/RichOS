//! RichOS document search: the shared text list, Qdrant Edge meaning search, and Super
//! Finder's fusion of the two (search PRD v3.1, richos-hq
//! `docs/prds/2026-10-13-qdrant-edge-semantic-search.md`).
//!
//! The CEO's measure for all of it: *"a search in RichOS that's not only super fast but also
//! allows the user (and Rich) to easily and quickly find the right things even when the exact
//! term or name they are looking for is unknown or forgotten."*
//!
//! - [`text`] and [`query`]: folding, words and quoted spans.
//! - [`chunk`] and [`corpus`]: the included folder as documents and short passages.
//! - [`rank`]: the one text list both modes share, reciprocal rank fusion, the literal boost.
//! - [`embed`], [`vector`] and [`manifest`]: the meaning half, on Qdrant Edge.
//! - [`service`]: what the palette and Rich's tool call, with truthful status and fallback.
pub mod chunk;
pub mod corpus;
pub mod embed;
pub mod manifest;
pub mod query;
pub mod rank;
pub mod service;
pub mod text;
pub mod vector;

pub use service::{Config, DocHit, SearchRequest, SearchResponse, SearchService, Status, Tuning};
