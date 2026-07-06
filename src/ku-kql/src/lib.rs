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
pub mod parser;
pub mod executor;
#[cfg(feature = "storage")]
pub mod storage;
#[cfg(feature = "storage")]
pub mod graph_storage;

// Re-export graph types for convenience
#[cfg(feature = "storage")]
pub use graph_storage::GraphStorage;
