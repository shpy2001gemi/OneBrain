//! # onebrain-blob-store — persistent Base blob storage
//!
//! Hybrid redb + filesystem blob storage (`blob_storage`, owner of the Base
//! `TX-BLOB-001` transaction boundary) and the versioned spilled-blob
//! filesystem layout with its fail-closed migration (`blob_layout`).
//!
//! ARCH-11 slice 3 (D-051) moved both modules verbatim out of `ku-kql`: they
//! contain no KQL code and depend only on `ku-core`, `redb`, `fs2`, `serde` and
//! `blake3`, so blob storage no longer pulls the query language crate along.

pub mod blob_layout;
pub mod blob_storage;
