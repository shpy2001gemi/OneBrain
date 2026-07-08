//! UPnP automatic port forwarding.
//!
//! Best-effort: if UPnP is not available (router doesn't support,
//! or crate fails), we continue without it.
//! The seed relay will handle connectivity for NAT-traversal.

use std::net::SocketAddr;

/// Result of a UPnP mapping attempt.
pub struct UpnpResult {
    /// Whether a port mapping was successfully created.
    pub mapped: bool,
    /// The external address (if mapping succeeded).
    pub external_addr: Option<SocketAddr>,
    /// Human-readable status message.
    pub message: String,
}

/// Try to map a port via UPnP. Never fails — returns result status.
///
/// This is a best-effort stub. The `igd-next` crate may not compile
/// on Windows GNU toolchain, so we start with a no-op implementation.
/// When UPnP is not available, seed relay handles connectivity.
pub async fn try_upnp_map(_port: u16) -> UpnpResult {
    // TODO: Try to use igd-next if available.
    // For now, since igd-next may not compile on Windows GNU,
    // just report that UPnP is not available and continue.
    // The seed relay will handle connectivity.

    UpnpResult {
        mapped: false,
        external_addr: None,
        message: "UPnP: Not available (using seed relay instead)".to_string(),
    }
}
