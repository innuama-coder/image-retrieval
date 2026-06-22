//! Domain model root.
//!
//! Each submodule defines a family of domain types aligned with
//! `docs/design/rust-implementation-design.md`, the HLD module map,
//! and the v1.1 LLD.

pub mod candidate;
pub mod config;
pub mod delivery;
pub mod image;
pub mod metrics;
pub mod policy;
pub mod query_plan;
pub mod retrieval;
pub mod search;
