//! # ku-kql — Knowledge Query Language
//!
//! Parser, executor, and persistent storage for the OBP query language.
//!
//! ## Modules
//! - **ast**: Query AST (Abstract Syntax Tree) types
//! - **parser**: nom-based KQL parser
//! - **read_only**: local read-only gate for user-facing textual KQL (ARCH-04)
//! - **executor**: Local query execution engine (operates on KuRuntime)
//! - **storage** feature: redb persistence for the private/standing need stores.
//!   Blob storage moved to the `onebrain-blob-store` crate (ARCH-11 slice 3) and
//!   legacy `KuStorage`/`GraphStorage` to `onebrain-legacy-ku-store` (slice 4).
//! - **experimental** feature: library-only typed KQL modules with no production
//!   consumer (OBKG projection, assembly search, relational alignment, structural
//!   signatures; ARCH-11 slice 1) and the private multipath island used only by
//!   the node M5 benchmark companion (multipath, query view, exploration, route
//!   packet, disclosure capsule; slice 2). Not in the default build.

pub mod ast;
pub mod executor;
pub mod parser;
pub mod read_only;
#[cfg(feature = "experimental")]
pub mod vnext_assembly_search;
pub mod vnext_disclosure;
#[cfg(feature = "experimental")]
pub mod vnext_disclosure_capsule;
#[cfg(feature = "experimental")]
pub mod vnext_exploration;
pub mod vnext_matcher;
#[cfg(feature = "experimental")]
pub mod vnext_multipath;
#[cfg(feature = "experimental")]
pub mod vnext_obkg_projection;
pub mod vnext_planner;
pub mod vnext_private_need;
pub mod vnext_proposal;
pub mod vnext_query;
#[cfg(feature = "experimental")]
pub mod vnext_query_view;
#[cfg(feature = "experimental")]
pub mod vnext_relational_alignment;
pub mod vnext_reunion;
#[cfg(feature = "experimental")]
pub mod vnext_route_packet;
pub mod vnext_semantic_index;
pub mod vnext_standing_need;
#[cfg(feature = "experimental")]
pub mod vnext_structural_signature;
