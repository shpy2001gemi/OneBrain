//! # Distributed Query Engine — Phase C+D
//!
//! Wires KQL queries to the network layer:
//! - **ConceptIndex**: Maps concept IDs to DHT keys for network lookup.
//! - **QueryRouter**: 6-layer scope escalation (Local → Neighbors → Cluster → DHT → Semantic → Global).
//! - **ResultMerger**: Deduplicates, ranks, and merges results from distributed queries.
//! - **QueryMessage**: Wire format for query forwarding/responses.

pub mod cache;
pub mod discovery;
pub mod index;
pub mod learning;
pub mod merger;
pub mod messages;
pub mod router;
pub mod watch;
