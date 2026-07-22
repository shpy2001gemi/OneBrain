//! OneBrain API — REST/WebSocket server interface.

pub mod error;
pub mod handlers;
pub mod server;
pub mod types;

pub use server::ApiServer;
