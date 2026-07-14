//! OneBrain Node — shared runtime for all UI interfaces.
//!
//! This crate contains the core node logic: OneBrainNode, networking,
//! configuration, peer management, seed client, and verification.
//! All interface projects (CLI, Web, Desktop, Mobile) depend on this.

pub mod config;
pub mod node;
pub mod error;
pub mod types;
pub mod display;
pub mod anti_gaming_guard;
pub mod network;
pub mod peer_manager;
pub mod verifier_service;
pub mod upnp;
pub mod mdns_discovery;
pub mod seed_client;
pub mod peer_memory;

pub use config::NodeConfig;
pub use node::{OneBrainNode, EncodeStoreResult};
pub use error::NodeError;
pub use network::{NodeEvent, PeerInfo, NetMessage};
