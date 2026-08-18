//! Relay-wide resource budgets shared by every authenticated outer connection.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

use ed25519_dalek::pkcs8::EncodePrivateKey;
use ed25519_dalek::SigningKey;
use onebrain_protocol::RelayDescriptorV1;
use rcgen::{CertificateParams, KeyPair, PKCS_ED25519};
use rustls::pki_types::PrivatePkcs8KeyDer;

use crate::RelayDataPlaneError;

pub const GLOBAL_QUEUE_FRAMES: usize = 1_024;
pub const GLOBAL_QUEUE_BYTES: usize = 16 * 1024 * 1024;
pub const GLOBAL_REASSEMBLIES: usize = 512;
pub const GLOBAL_REASSEMBLY_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_PENDING_OUTER_HANDSHAKES: usize = 64;
pub const MAX_ACTIVE_OUTER_CONNECTIONS: usize = 256;
pub const MAX_OUTER_CONNECTIONS_PER_SOURCE: usize = 8;
pub const MAX_PREAUTH_BYTES_PER_SOURCE: usize = 65_536;
pub const MAX_SOURCE_COUNTER_ENTRIES: usize = 1_024;

#[derive(Clone, Debug)]
pub struct OuterConnectionLimiter {
    inner: Arc<Mutex<ConnectionLimitState>>,
    pending_limit: usize,
    active_limit: usize,
    per_source_limit: usize,
    preauth_byte_limit: usize,
}

#[derive(Debug, Default)]
struct ConnectionLimitState {
    pending: usize,
    active: usize,
    sources: BTreeMap<IpAddr, SourceLimitState>,
}

#[derive(Clone, Copy, Debug, Default)]
struct SourceLimitState {
    pending: usize,
    active: usize,
    preauth_bytes: usize,
}

#[derive(Debug)]
pub struct PendingOuterAdmission {
    limiter: OuterConnectionLimiter,
    source: IpAddr,
    preauth_bytes: usize,
    live: bool,
}

#[derive(Debug)]
pub struct ActiveOuterAdmission {
    limiter: OuterConnectionLimiter,
    source: IpAddr,
    live: bool,
}

impl OuterConnectionLimiter {
    pub fn standard() -> Self {
        Self::with_limits(
            MAX_PENDING_OUTER_HANDSHAKES,
            MAX_ACTIVE_OUTER_CONNECTIONS,
            MAX_OUTER_CONNECTIONS_PER_SOURCE,
            MAX_PREAUTH_BYTES_PER_SOURCE,
        )
    }

    pub fn with_limits(
        pending_limit: usize,
        active_limit: usize,
        per_source_limit: usize,
        preauth_byte_limit: usize,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ConnectionLimitState::default())),
            pending_limit,
            active_limit,
            per_source_limit,
            preauth_byte_limit,
        }
    }

    pub fn begin(
        &self,
        source: IpAddr,
        preauth_bytes: usize,
    ) -> Result<PendingOuterAdmission, RelayDataPlaneError> {
        if preauth_bytes == 0 || preauth_bytes > self.preauth_byte_limit {
            return Err(RelayDataPlaneError::Capacity);
        }
        let mut state = self.inner.lock().map_err(|_| RelayDataPlaneError::Closed)?;
        if state.pending >= self.pending_limit
            || (!state.sources.contains_key(&source)
                && state.sources.len() >= MAX_SOURCE_COUNTER_ENTRIES)
        {
            return Err(RelayDataPlaneError::Capacity);
        }
        let source_state = state.sources.entry(source).or_default();
        if source_state.pending + source_state.active >= self.per_source_limit
            || source_state
                .preauth_bytes
                .checked_add(preauth_bytes)
                .is_none_or(|value| value > self.preauth_byte_limit)
        {
            return Err(RelayDataPlaneError::Capacity);
        }
        source_state.pending += 1;
        source_state.preauth_bytes += preauth_bytes;
        state.pending += 1;
        drop(state);
        Ok(PendingOuterAdmission {
            limiter: self.clone(),
            source,
            preauth_bytes,
            live: true,
        })
    }
}

impl PendingOuterAdmission {
    pub fn promote(mut self) -> Result<ActiveOuterAdmission, RelayDataPlaneError> {
        let mut state = self
            .limiter
            .inner
            .lock()
            .map_err(|_| RelayDataPlaneError::Closed)?;
        if state.active >= self.limiter.active_limit {
            return Err(RelayDataPlaneError::Capacity);
        }
        let source = state
            .sources
            .get_mut(&self.source)
            .ok_or(RelayDataPlaneError::Closed)?;
        source.pending = source.pending.saturating_sub(1);
        source.preauth_bytes = source.preauth_bytes.saturating_sub(self.preauth_bytes);
        source.active += 1;
        state.pending = state.pending.saturating_sub(1);
        state.active += 1;
        self.live = false;
        drop(state);
        Ok(ActiveOuterAdmission {
            limiter: self.limiter.clone(),
            source: self.source,
            live: true,
        })
    }
}

impl Drop for PendingOuterAdmission {
    fn drop(&mut self) {
        if !self.live {
            return;
        }
        if let Ok(mut state) = self.limiter.inner.lock() {
            state.pending = state.pending.saturating_sub(1);
            release_source(&mut state, self.source, true, self.preauth_bytes);
        }
    }
}

impl Drop for ActiveOuterAdmission {
    fn drop(&mut self) {
        if !self.live {
            return;
        }
        if let Ok(mut state) = self.limiter.inner.lock() {
            state.active = state.active.saturating_sub(1);
            release_source(&mut state, self.source, false, 0);
        }
    }
}

fn release_source(
    state: &mut ConnectionLimitState,
    source: IpAddr,
    pending: bool,
    preauth_bytes: usize,
) {
    let remove = if let Some(source_state) = state.sources.get_mut(&source) {
        if pending {
            source_state.pending = source_state.pending.saturating_sub(1);
            source_state.preauth_bytes = source_state.preauth_bytes.saturating_sub(preauth_bytes);
        } else {
            source_state.active = source_state.active.saturating_sub(1);
        }
        source_state.pending == 0 && source_state.active == 0 && source_state.preauth_bytes == 0
    } else {
        false
    };
    if remove {
        state.sources.remove(&source);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayIdentityCertificate {
    certificate_der: Vec<u8>,
    private_key_der: Vec<u8>,
    spki_ed25519: [u8; 32],
}

impl RelayIdentityCertificate {
    pub fn certificate_der(&self) -> &[u8] {
        &self.certificate_der
    }

    pub fn private_key_der(&self) -> &[u8] {
        &self.private_key_der
    }

    pub fn spki_ed25519(&self) -> [u8; 32] {
        self.spki_ed25519
    }
}

pub fn install_aws_lc_provider() -> Result<(), RelayDataPlaneError> {
    if rustls::crypto::CryptoProvider::get_default().is_some() {
        return Ok(());
    }
    match rustls::crypto::aws_lc_rs::default_provider().install_default() {
        Ok(()) => Ok(()),
        Err(_) if rustls::crypto::CryptoProvider::get_default().is_some() => Ok(()),
        Err(_) => Err(RelayDataPlaneError::IdentityMismatch),
    }
}

pub fn relay_identity_certificate(
    signer: &SigningKey,
    descriptor: &RelayDescriptorV1,
) -> Result<RelayIdentityCertificate, RelayDataPlaneError> {
    install_aws_lc_provider()?;
    let public = *signer.verifying_key().as_bytes();
    if descriptor.relay_public_key != public
        || crate::principal_node_id(&public) != descriptor.relay_node_id
    {
        return Err(RelayDataPlaneError::IdentityMismatch);
    }
    let pkcs8 = signer
        .to_pkcs8_der()
        .map_err(|_| RelayDataPlaneError::IdentityMismatch)?;
    let key_pair = KeyPair::from_pkcs8_der_and_sign_algo(
        &PrivatePkcs8KeyDer::from(pkcs8.as_bytes()),
        &PKCS_ED25519,
    )
    .map_err(|_| RelayDataPlaneError::IdentityMismatch)?;
    let spki_ed25519: [u8; 32] = key_pair
        .public_key_raw()
        .try_into()
        .map_err(|_| RelayDataPlaneError::IdentityMismatch)?;
    if spki_ed25519 != public {
        return Err(RelayDataPlaneError::IdentityMismatch);
    }
    let certificate = CertificateParams::new(vec!["relay.onebrain".to_string()])
        .map_err(|_| RelayDataPlaneError::IdentityMismatch)?
        .self_signed(&key_pair)
        .map_err(|_| RelayDataPlaneError::IdentityMismatch)?;
    Ok(RelayIdentityCertificate {
        certificate_der: certificate.der().to_vec(),
        private_key_der: key_pair.serialize_der(),
        spki_ed25519,
    })
}

#[derive(Clone, Debug)]
pub struct RelayGlobalBudget {
    inner: Arc<Mutex<BudgetState>>,
    queue_frame_limit: usize,
    queue_byte_limit: usize,
    reassembly_limit: usize,
    reassembly_byte_limit: usize,
}

#[derive(Debug, Default)]
struct BudgetState {
    queue_frames: usize,
    queue_bytes: usize,
    reassemblies: usize,
    reassembly_bytes: usize,
}

#[derive(Debug)]
pub struct BudgetLease {
    budget: RelayGlobalBudget,
    kind: BudgetKind,
    count: usize,
    bytes: usize,
}

#[derive(Clone, Copy, Debug)]
enum BudgetKind {
    Queue,
    Reassembly,
}

impl RelayGlobalBudget {
    pub fn standard() -> Self {
        Self::new_for_test(
            GLOBAL_QUEUE_FRAMES,
            GLOBAL_QUEUE_BYTES,
            GLOBAL_REASSEMBLIES,
            GLOBAL_REASSEMBLY_BYTES,
        )
    }

    pub fn new_for_test(
        queue_frame_limit: usize,
        queue_byte_limit: usize,
        reassembly_limit: usize,
        reassembly_byte_limit: usize,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(BudgetState::default())),
            queue_frame_limit,
            queue_byte_limit,
            reassembly_limit,
            reassembly_byte_limit,
        }
    }

    pub fn reserve_queue(
        &self,
        count: usize,
        bytes: usize,
    ) -> Result<BudgetLease, RelayDataPlaneError> {
        self.reserve(BudgetKind::Queue, count, bytes)
    }

    pub(crate) fn reserve_reassembly(
        &self,
        bytes: usize,
    ) -> Result<BudgetLease, RelayDataPlaneError> {
        self.reserve(BudgetKind::Reassembly, 1, bytes)
    }

    fn reserve(
        &self,
        kind: BudgetKind,
        count: usize,
        bytes: usize,
    ) -> Result<BudgetLease, RelayDataPlaneError> {
        if count == 0 || bytes == 0 {
            return Err(RelayDataPlaneError::Capacity);
        }
        let mut state = self.inner.lock().map_err(|_| RelayDataPlaneError::Closed)?;
        let (next_count, next_bytes, count_limit, byte_limit) = match kind {
            BudgetKind::Queue => (
                state.queue_frames.checked_add(count),
                state.queue_bytes.checked_add(bytes),
                self.queue_frame_limit,
                self.queue_byte_limit,
            ),
            BudgetKind::Reassembly => (
                state.reassemblies.checked_add(count),
                state.reassembly_bytes.checked_add(bytes),
                self.reassembly_limit,
                self.reassembly_byte_limit,
            ),
        };
        let (Some(next_count), Some(next_bytes)) = (next_count, next_bytes) else {
            return Err(RelayDataPlaneError::Capacity);
        };
        if next_count > count_limit || next_bytes > byte_limit {
            return Err(RelayDataPlaneError::Capacity);
        }
        match kind {
            BudgetKind::Queue => {
                state.queue_frames = next_count;
                state.queue_bytes = next_bytes;
            }
            BudgetKind::Reassembly => {
                state.reassemblies = next_count;
                state.reassembly_bytes = next_bytes;
            }
        }
        drop(state);
        Ok(BudgetLease {
            budget: self.clone(),
            kind,
            count,
            bytes,
        })
    }
}

impl Drop for BudgetLease {
    fn drop(&mut self) {
        let Ok(mut state) = self.budget.inner.lock() else {
            return;
        };
        match self.kind {
            BudgetKind::Queue => {
                state.queue_frames = state.queue_frames.saturating_sub(self.count);
                state.queue_bytes = state.queue_bytes.saturating_sub(self.bytes);
            }
            BudgetKind::Reassembly => {
                state.reassemblies = state.reassemblies.saturating_sub(self.count);
                state.reassembly_bytes = state.reassembly_bytes.saturating_sub(self.bytes);
            }
        }
    }
}
