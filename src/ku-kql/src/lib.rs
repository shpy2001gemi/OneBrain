//! # ku-kql — Knowledge Query Language
//!
//! Parser, executor, and persistent storage for the OBP query language.
//!
//! ## Modules
//! - **ast**: Query AST (Abstract Syntax Tree) types
//! - **parser**: nom-based KQL parser
//! - **executor**: Local query execution engine (operates on KuRuntime)
//! - **storage**: redb-backed persistent KU storage

pub mod ast;
#[cfg(feature = "storage")]
pub mod blob_storage;
pub mod executor;
#[cfg(feature = "storage")]
pub mod graph_storage;
pub mod parser;
#[cfg(feature = "storage")]
pub mod storage;
pub mod vnext_assembly_search;
pub mod vnext_disclosure;
pub mod vnext_disclosure_capsule;
pub mod vnext_exploration;
pub mod vnext_matcher;
pub mod vnext_multipath;
pub mod vnext_obkg_projection;
pub mod vnext_planner;
pub mod vnext_proposal;
pub mod vnext_query;
pub mod vnext_query_view;
pub mod vnext_relational_alignment;
pub mod vnext_reunion;
pub mod vnext_route_packet;
pub mod vnext_semantic_index;
pub mod vnext_standing_need;
pub mod vnext_structural_signature;

// Re-export graph types for convenience
#[cfg(feature = "storage")]
pub use graph_storage::GraphStorage;
