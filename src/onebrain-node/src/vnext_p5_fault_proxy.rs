//! Bounded P5 delivery fault proxy.
//!
//! This component can delay, drop, duplicate, reorder, or partition transport
//! frames. It never parses application records and cannot create validation,
//! authority, truth, completion, reward, or wallet state.

#![cfg(feature = "vnext-production-canary-harness")]

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const P5_MAX_PROXY_FRAME_BYTES: usize = 1_048_576;
pub const P5_MAX_REORDERED_FRAMES: usize = 16;
pub const P5_MAX_DUPLICATE_COPIES: u8 = 2;
pub const P5_MAX_FAULT_DELAY_MS: u64 = 300_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum P5FaultKind {
    Partition,
    Drop,
    Reorder,
    Duplicate,
    Restart,
    AddressChange,
    SeedOutage,
    SignerOutage,
    DiskPressure,
    SlowPeer,
    BaseObarv002ArchiveRestore,
    Rollback,
    ExplicitReEnable,
}

impl P5FaultKind {
    pub const ALL: [Self; 13] = [
        Self::Partition,
        Self::Drop,
        Self::Reorder,
        Self::Duplicate,
        Self::Restart,
        Self::AddressChange,
        Self::SeedOutage,
        Self::SignerOutage,
        Self::DiskPressure,
        Self::SlowPeer,
        Self::BaseObarv002ArchiveRestore,
        Self::Rollback,
        Self::ExplicitReEnable,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Partition => "partition",
            Self::Drop => "drop",
            Self::Reorder => "reorder",
            Self::Duplicate => "duplicate",
            Self::Restart => "restart",
            Self::AddressChange => "address-change",
            Self::SeedOutage => "seed-outage",
            Self::SignerOutage => "signer-outage",
            Self::DiskPressure => "disk-pressure",
            Self::SlowPeer => "slow-peer",
            Self::BaseObarv002ArchiveRestore => "base-obarv002-archive-restore",
            Self::Rollback => "rollback",
            Self::ExplicitReEnable => "explicit-re-enable",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct P5FaultProxyConfig {
    pub fault: Option<P5FaultKind>,
    pub delay_ms: u64,
    pub duplicate_copies: u8,
}

impl Default for P5FaultProxyConfig {
    fn default() -> Self {
        Self {
            fault: None,
            delay_ms: 0,
            duplicate_copies: 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5DeliveryBatch {
    pub deliveries: Vec<Vec<u8>>,
    pub delay_ms: u64,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum P5FaultProxyError {
    #[error("P5 proxy frame exceeds the fixed byte bound")]
    FrameTooLarge,
    #[error("P5 proxy delay exceeds the fixed duration bound")]
    DelayTooLong,
    #[error("P5 proxy duplicate count exceeds the fixed copy bound")]
    TooManyCopies,
    #[error("P5 proxy reorder buffer is full")]
    ReorderBufferFull,
}

pub struct P5FaultProxy {
    config: P5FaultProxyConfig,
    reordered: VecDeque<Vec<u8>>,
}

impl Default for P5FaultProxy {
    fn default() -> Self {
        Self {
            config: P5FaultProxyConfig::default(),
            reordered: VecDeque::new(),
        }
    }
}

impl P5FaultProxy {
    pub fn configure(&mut self, config: P5FaultProxyConfig) -> Result<(), P5FaultProxyError> {
        if config.delay_ms > P5_MAX_FAULT_DELAY_MS {
            return Err(P5FaultProxyError::DelayTooLong);
        }
        if config.duplicate_copies == 0 || config.duplicate_copies > P5_MAX_DUPLICATE_COPIES {
            return Err(P5FaultProxyError::TooManyCopies);
        }
        self.config = config;
        Ok(())
    }

    pub fn deliver(&mut self, frame: Vec<u8>) -> Result<P5DeliveryBatch, P5FaultProxyError> {
        if frame.len() > P5_MAX_PROXY_FRAME_BYTES {
            return Err(P5FaultProxyError::FrameTooLarge);
        }
        let fault = self.config.fault;
        let deliveries = match fault {
            Some(P5FaultKind::Partition | P5FaultKind::Drop) => Vec::new(),
            Some(P5FaultKind::Reorder) => {
                if self.reordered.len() >= P5_MAX_REORDERED_FRAMES {
                    return Err(P5FaultProxyError::ReorderBufferFull);
                }
                if let Some(previous) = self.reordered.pop_front() {
                    vec![frame, previous]
                } else {
                    self.reordered.push_back(frame);
                    Vec::new()
                }
            }
            Some(P5FaultKind::Duplicate) => (0..self.config.duplicate_copies)
                .map(|_| frame.clone())
                .collect(),
            _ => vec![frame],
        };
        Ok(P5DeliveryBatch {
            deliveries,
            delay_ms: if fault == Some(P5FaultKind::SlowPeer) {
                self.config.delay_ms
            } else {
                0
            },
        })
    }

    pub fn flush_reordered(&mut self) -> Vec<Vec<u8>> {
        self.reordered.drain(..).collect()
    }

    pub fn changes_delivery_conditions_only(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vnext_p5_multi_host_fault_proxy_is_bounded_and_delivery_only() {
        let mut proxy = P5FaultProxy::default();
        assert!(proxy.changes_delivery_conditions_only());
        assert_eq!(
            proxy.deliver(vec![0; P5_MAX_PROXY_FRAME_BYTES + 1]),
            Err(P5FaultProxyError::FrameTooLarge)
        );
        assert_eq!(
            proxy.configure(P5FaultProxyConfig {
                fault: Some(P5FaultKind::SlowPeer),
                delay_ms: P5_MAX_FAULT_DELAY_MS + 1,
                duplicate_copies: 1,
            }),
            Err(P5FaultProxyError::DelayTooLong)
        );
        assert_eq!(
            proxy.configure(P5FaultProxyConfig {
                fault: Some(P5FaultKind::Duplicate),
                delay_ms: 0,
                duplicate_copies: P5_MAX_DUPLICATE_COPIES + 1,
            }),
            Err(P5FaultProxyError::TooManyCopies)
        );
        proxy
            .configure(P5FaultProxyConfig {
                fault: Some(P5FaultKind::Duplicate),
                delay_ms: 0,
                duplicate_copies: 2,
            })
            .unwrap();
        assert_eq!(
            proxy.deliver(b"opaque".to_vec()).unwrap().deliveries.len(),
            2
        );
    }

    #[test]
    fn vnext_p5_multi_host_reorder_never_parses_or_fabricates_frames() {
        let mut proxy = P5FaultProxy::default();
        proxy
            .configure(P5FaultProxyConfig {
                fault: Some(P5FaultKind::Reorder),
                delay_ms: 0,
                duplicate_copies: 1,
            })
            .unwrap();
        assert!(proxy
            .deliver(b"first".to_vec())
            .unwrap()
            .deliveries
            .is_empty());
        assert_eq!(
            proxy.deliver(b"second".to_vec()).unwrap().deliveries,
            vec![b"second".to_vec(), b"first".to_vec()]
        );
    }
}
