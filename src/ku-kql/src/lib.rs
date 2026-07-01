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
