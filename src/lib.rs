//! image-retrieval library root.
//!
//! This crate provides the domain model, port definitions, error diagnostics,
//! and module boundaries for the image-retrieval Rust CLI.
//!
//! # Module map (aligned with `docs/design/rust-implementation-design.md`)
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`domain`] | Core domain types: QueryPlan, Candidate, Retrieval, Image, Delivery, Policy, Metrics |
//! | [`error`] | Error families and diagnostic models |
//! | [`ports`] | `BaseProvider`, `BaseRetrievalChannel`, and `OpenClawEvaluationPort` trait boundaries |
//! | [`quality`] | Candidate and image quality gate type definitions |
//! | [`orchestrator`] | Task orchestrator and state machine placeholder |
//! | [`delivery`] | Delivery package builder placeholder |
//! | [`policy`] | Policy and guardrails placeholder |
//! | [`observability`] | Task evidence and metrics placeholder |
//! | [`self_check`] | Readiness self-check placeholder |

pub mod domain;
pub mod error;
pub mod ports;
pub mod search;

// Downstream module placeholders — type definitions only, no production logic.
pub mod delivery;
pub mod observability;
pub mod orchestrator;
pub mod policy;
pub mod quality;
pub mod self_check;
