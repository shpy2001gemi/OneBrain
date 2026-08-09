//! OneBrain API — REST/WebSocket server interface.

#[cfg(all(feature = "legacy-read-compat", not(feature = "base-v1")))]
compile_error!("legacy-read-compat requires base-v1");

pub mod error;
pub mod handlers;
pub mod server;
pub mod types;
pub mod vnext_api;
pub mod vnext_ws;

pub use server::{base_runtime_config_for_api_token, ApiServer};
#[cfg(feature = "vnext-network-runtime")]
pub use vnext_api::VNextFeedPublisher;
