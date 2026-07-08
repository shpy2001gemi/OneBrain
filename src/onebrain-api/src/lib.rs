//! OneBrain API — REST/WebSocket server interface.

pub mod server;
pub mod handlers;
pub mod types;
pub mod error;

pub use server::ApiServer;
