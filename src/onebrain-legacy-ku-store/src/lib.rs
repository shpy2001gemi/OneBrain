//! # onebrain-legacy-ku-store — legacy Core DNA KU storage
//!
//! redb-backed persistent storage for legacy `KuRuntime` (Core DNA wire bytes
//! plus Epigenetics; `storage::KuStorage`) and the OBKG graph edge index it
//! composes (`graph_storage::GraphStorage`). The node opens it through the
//! read-only Base compatibility port and the legacy migration-evidence scan.
//!
//! ARCH-11 slice 4 (D-051) moved both modules verbatim out of `ku-kql`: they
//! contain no KQL code and depend only on `ku-core`, `redb`, `serde_json` and
//! `blake3`, so legacy KU storage no longer lives in the query language crate.

pub mod graph_storage;
pub mod storage;

// Re-export graph types for convenience
pub use graph_storage::GraphStorage;
