//! Bounded, process-local archive capability custody.
//!
//! Raw archive bytes and credentials never enter the product facade. Callers
//! receive opaque, reservation-bound handles; the registry is the sole owner
//! of their bounded buffers and zeroizing secret material.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{Arc, Mutex, Weak};

use onebrain_archive::{ArchiveCredential, ArchiveCredentialKind, RecoveryKey};
use zeroize::Zeroizing;

use crate::error::NodeError;

pub const MAX_ARCHIVE_CAPABILITIES: usize = 64;
pub const MAX_ARCHIVE_RESERVATIONS: usize = 32;
pub const MAX_ARCHIVE_CHUNK_BYTES: usize = 1024 * 1024;
pub const MAX_ARCHIVE_PASSWORD_BYTES: usize = 1024;
pub const DEFAULT_ARCHIVE_SPOOL_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArchiveCapabilityId([u8; 32]);

impl ArchiveCapabilityId {
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArchiveOperationReservationId([u8; 32]);

impl ArchiveOperationReservationId {
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchiveProcessGeneration([u8; 32]);

impl ArchiveProcessGeneration {
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy)]
struct CapabilityBinding {
    id: ArchiveCapabilityId,
    reservation: ArchiveOperationReservationId,
    process_generation: ArchiveProcessGeneration,
}

struct CapabilityHandle {
    binding: CapabilityBinding,
    registry: Weak<RegistryInner>,
    armed: bool,
}

impl CapabilityHandle {
    fn new(binding: CapabilityBinding, registry: &Arc<RegistryInner>) -> Self {
        Self {
            binding,
            registry: Arc::downgrade(registry),
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for CapabilityHandle {
    fn drop(&mut self) {
        if self.armed {
            if let Some(registry) = self.registry.upgrade() {
                registry.remove_best_effort(self.binding.id);
            }
        }
    }
}

macro_rules! capability_handle {
    ($name:ident) => {
        pub struct $name(CapabilityHandle);

        impl $name {
            pub const fn id(&self) -> ArchiveCapabilityId {
                self.0.binding.id
            }

            pub const fn owner_reservation(&self) -> ArchiveOperationReservationId {
                self.0.binding.reservation
            }

            pub const fn process_generation(&self) -> ArchiveProcessGeneration {
                self.0.binding.process_generation
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_struct(stringify!($name))
                    .field("id", &self.id())
                    .field("owner_reservation", &self.owner_reservation())
                    .field("process_generation", &self.process_generation())
                    .finish()
            }
        }
    };
}

capability_handle!(WritableArchiveSourceHandle);
capability_handle!(SealedArchiveSourceHandle);
capability_handle!(WritableArchiveSinkHandle);
capability_handle!(ReadableArchiveSinkHandle);
capability_handle!(ArchiveSecretHandle);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundedArchiveChunk {
    pub offset: u64,
    pub bytes: Vec<u8>,
    pub eof: bool,
}

enum CapabilityValue {
    SourceWriting {
        expected_bytes: u64,
        next_offset: u64,
        bytes: Vec<u8>,
    },
    SourceSealed {
        exact_bytes: u64,
        bytes: Vec<u8>,
    },
    SinkWriting {
        maximum_bytes: u64,
    },
    SinkReadable {
        maximum_bytes: u64,
        bytes: Vec<u8>,
    },
    Secret {
        kind: ArchiveCredentialKind,
        bytes: Zeroizing<Vec<u8>>,
    },
}

impl CapabilityValue {
    const fn reserved_bytes(&self) -> u64 {
        match self {
            Self::SourceWriting { expected_bytes, .. } => *expected_bytes,
            Self::SourceSealed { exact_bytes, .. } => *exact_bytes,
            Self::SinkWriting { maximum_bytes } | Self::SinkReadable { maximum_bytes, .. } => {
                *maximum_bytes
            }
            Self::Secret { .. } => 0,
        }
    }
}

struct CapabilityRecord {
    binding: CapabilityBinding,
    value: CapabilityValue,
}

struct RegistryState {
    reservations: BTreeSet<ArchiveOperationReservationId>,
    capabilities: BTreeMap<ArchiveCapabilityId, CapabilityRecord>,
    reserved_spool_bytes: u64,
}

struct RegistryInner {
    process_generation: ArchiveProcessGeneration,
    max_spool_bytes: u64,
    state: Mutex<RegistryState>,
}

impl RegistryInner {
    fn remove_best_effort(&self, id: ArchiveCapabilityId) {
        if let Ok(mut state) = self.state.lock() {
            remove_record(&mut state, id);
        }
    }
}

#[derive(Clone)]
pub struct ArchiveCapabilityRegistry {
    inner: Arc<RegistryInner>,
}

impl ArchiveCapabilityRegistry {
    pub fn new() -> Result<Self, NodeError> {
        Self::with_spool_limit(DEFAULT_ARCHIVE_SPOOL_BYTES)
    }

    pub fn with_spool_limit(max_spool_bytes: u64) -> Result<Self, NodeError> {
        if max_spool_bytes == 0 {
            return Err(capability_error("archive spool limit must be nonzero"));
        }
        Ok(Self {
            inner: Arc::new(RegistryInner {
                process_generation: ArchiveProcessGeneration(random_id()?),
                max_spool_bytes,
                state: Mutex::new(RegistryState {
                    reservations: BTreeSet::new(),
                    capabilities: BTreeMap::new(),
                    reserved_spool_bytes: 0,
                }),
            }),
        })
    }

    pub fn process_generation(&self) -> ArchiveProcessGeneration {
        self.inner.process_generation
    }

    /// Task 13's bounded internal reservation registry. Task 17 replaces this
    /// entry point with the durable Base operation reservation.
    pub fn reserve_operation(&self) -> Result<ArchiveOperationReservationId, NodeError> {
        let mut state = self.lock_state()?;
        if state.reservations.len() >= MAX_ARCHIVE_RESERVATIONS {
            return Err(capability_error("archive reservation limit reached"));
        }
        for _ in 0..32 {
            let id = ArchiveOperationReservationId(random_id()?);
            if state.reservations.insert(id) {
                return Ok(id);
            }
        }
        Err(capability_error("archive reservation ID collision"))
    }

    pub fn release_reservation(
        &self,
        reservation: ArchiveOperationReservationId,
    ) -> Result<(), NodeError> {
        let mut state = self.lock_state()?;
        if !state.reservations.remove(&reservation) {
            return Err(capability_error("unknown archive reservation"));
        }
        let ids = state
            .capabilities
            .iter()
            .filter_map(|(id, record)| (record.binding.reservation == reservation).then_some(*id))
            .collect::<Vec<_>>();
        for id in ids {
            remove_record(&mut state, id);
        }
        Ok(())
    }

    pub fn begin_source(
        &self,
        owner_reservation: ArchiveOperationReservationId,
        expected_encrypted_bytes: u64,
    ) -> Result<WritableArchiveSourceHandle, NodeError> {
        if expected_encrypted_bytes == 0 {
            return Err(capability_error("archive source length must be nonzero"));
        }
        let binding = self.insert_capability(
            owner_reservation,
            CapabilityValue::SourceWriting {
                expected_bytes: expected_encrypted_bytes,
                next_offset: 0,
                bytes: Vec::new(),
            },
        )?;
        Ok(WritableArchiveSourceHandle(CapabilityHandle::new(
            binding,
            &self.inner,
        )))
    }

    pub fn push_source_chunk(
        &self,
        handle: &WritableArchiveSourceHandle,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), NodeError> {
        if bytes.is_empty() || bytes.len() > MAX_ARCHIVE_CHUNK_BYTES {
            return Err(capability_error("archive source chunk is out of bounds"));
        }
        let mut state = self.lock_state()?;
        let record = validate_record_mut(&mut state, &handle.0.binding, self.process_generation())?;
        match &mut record.value {
            CapabilityValue::SourceWriting {
                expected_bytes,
                next_offset,
                bytes: stored,
            } => {
                let end = offset
                    .checked_add(bytes.len() as u64)
                    .ok_or_else(|| capability_error("archive source offset overflow"))?;
                if offset != *next_offset || end > *expected_bytes {
                    return Err(capability_error("archive source offset is not contiguous"));
                }
                stored.extend_from_slice(bytes);
                *next_offset = end;
                Ok(())
            }
            _ => Err(capability_error(
                "archive source handle has the wrong state",
            )),
        }
    }

    pub fn seal_source(
        &self,
        mut handle: WritableArchiveSourceHandle,
    ) -> Result<SealedArchiveSourceHandle, NodeError> {
        let binding = handle.0.binding;
        let result = (|| {
            let mut state = self.lock_state()?;
            let record = validate_record_mut(&mut state, &binding, self.process_generation())?;
            let replacement = match std::mem::replace(
                &mut record.value,
                CapabilityValue::SourceWriting {
                    expected_bytes: 0,
                    next_offset: 0,
                    bytes: Vec::new(),
                },
            ) {
                CapabilityValue::SourceWriting {
                    expected_bytes,
                    next_offset,
                    bytes,
                } if next_offset == expected_bytes && bytes.len() as u64 == expected_bytes => {
                    CapabilityValue::SourceSealed {
                        exact_bytes: expected_bytes,
                        bytes,
                    }
                }
                _ => {
                    remove_record(&mut state, binding.id);
                    return Err(capability_error("archive source length is incomplete"));
                }
            };
            record.value = replacement;
            Ok(())
        })();
        handle.0.disarm();
        result?;
        Ok(SealedArchiveSourceHandle(CapabilityHandle::new(
            binding,
            &self.inner,
        )))
    }

    pub fn begin_sink(
        &self,
        owner_reservation: ArchiveOperationReservationId,
        max_encrypted_bytes: u64,
    ) -> Result<WritableArchiveSinkHandle, NodeError> {
        if max_encrypted_bytes == 0 {
            return Err(capability_error("archive sink limit must be nonzero"));
        }
        let binding = self.insert_capability(
            owner_reservation,
            CapabilityValue::SinkWriting {
                maximum_bytes: max_encrypted_bytes,
            },
        )?;
        Ok(WritableArchiveSinkHandle(CapabilityHandle::new(
            binding,
            &self.inner,
        )))
    }

    pub fn read_sink_chunk(
        &self,
        handle: &ReadableArchiveSinkHandle,
        offset: u64,
        max: u32,
    ) -> Result<BoundedArchiveChunk, NodeError> {
        let max = usize::try_from(max).map_err(|_| capability_error("invalid chunk bound"))?;
        if max == 0 || max > MAX_ARCHIVE_CHUNK_BYTES {
            return Err(capability_error("archive read chunk is out of bounds"));
        }
        let state = self.lock_state()?;
        let record = validate_record(&state, &handle.0.binding, self.process_generation())?;
        let bytes = match &record.value {
            CapabilityValue::SinkReadable { bytes, .. } => bytes,
            _ => return Err(capability_error("archive sink handle has the wrong state")),
        };
        let start = usize::try_from(offset).map_err(|_| capability_error("invalid read offset"))?;
        if start > bytes.len() {
            return Err(capability_error("archive read offset exceeds the sink"));
        }
        let end = start.saturating_add(max).min(bytes.len());
        Ok(BoundedArchiveChunk {
            offset,
            bytes: bytes[start..end].to_vec(),
            eof: end == bytes.len(),
        })
    }

    pub fn commit_sink(&self, mut handle: ReadableArchiveSinkHandle) -> Result<(), NodeError> {
        let id = handle.id();
        let mut state = self.lock_state()?;
        let record = validate_record(&state, &handle.0.binding, self.process_generation())?;
        if !matches!(record.value, CapabilityValue::SinkReadable { .. }) {
            return Err(capability_error("archive sink is not readable"));
        }
        remove_record(&mut state, id);
        handle.0.disarm();
        Ok(())
    }

    pub fn register_secret(
        &self,
        owner_reservation: ArchiveOperationReservationId,
        kind: ArchiveCredentialKind,
        secret: Zeroizing<Vec<u8>>,
    ) -> Result<ArchiveSecretHandle, NodeError> {
        match kind {
            ArchiveCredentialKind::Password
                if secret.is_empty() || secret.len() > MAX_ARCHIVE_PASSWORD_BYTES =>
            {
                return Err(capability_error("archive password is out of bounds"));
            }
            ArchiveCredentialKind::RecoveryKey if secret.len() != 32 => {
                return Err(capability_error(
                    "archive recovery key must be exactly 32 bytes",
                ));
            }
            _ => {}
        }
        let binding = self.insert_capability(
            owner_reservation,
            CapabilityValue::Secret {
                kind,
                bytes: secret,
            },
        )?;
        Ok(ArchiveSecretHandle(CapabilityHandle::new(
            binding,
            &self.inner,
        )))
    }

    pub fn abort(&self, id: ArchiveCapabilityId) -> Result<(), NodeError> {
        let mut state = self.lock_state()?;
        let record = state
            .capabilities
            .get(&id)
            .ok_or_else(|| capability_error("unknown archive capability"))?;
        if !matches!(
            record.value,
            CapabilityValue::SourceWriting { .. }
                | CapabilityValue::SinkWriting { .. }
                | CapabilityValue::Secret { .. }
        ) {
            return Err(capability_error(
                "sealed archive capability cannot be aborted",
            ));
        }
        remove_record(&mut state, id);
        Ok(())
    }

    pub fn destroy(&self, id: ArchiveCapabilityId) -> Result<(), NodeError> {
        let mut state = self.lock_state()?;
        if !state.capabilities.contains_key(&id) {
            return Err(capability_error("unknown archive capability"));
        }
        remove_record(&mut state, id);
        Ok(())
    }

    pub fn active_capability_count(&self) -> Result<usize, NodeError> {
        Ok(self.lock_state()?.capabilities.len())
    }

    pub(crate) fn take_source(
        &self,
        mut handle: SealedArchiveSourceHandle,
    ) -> Result<(ArchiveOperationReservationId, Vec<u8>), NodeError> {
        let binding = handle.0.binding;
        let mut state = self.lock_state()?;
        let record = validate_record(&state, &binding, self.process_generation())?;
        if !matches!(record.value, CapabilityValue::SourceSealed { .. }) {
            return Err(capability_error("archive source is not sealed"));
        }
        let record = remove_record(&mut state, binding.id)
            .ok_or_else(|| capability_error("unknown archive source"))?;
        handle.0.disarm();
        match record.value {
            CapabilityValue::SourceSealed { bytes, .. } => Ok((binding.reservation, bytes)),
            _ => Err(capability_error("archive source has the wrong state")),
        }
    }

    pub(crate) fn take_credential(
        &self,
        mut handle: ArchiveSecretHandle,
        expected_reservation: ArchiveOperationReservationId,
    ) -> Result<ArchiveCredential, NodeError> {
        let binding = handle.0.binding;
        if binding.reservation != expected_reservation {
            return Err(capability_error(
                "archive capabilities cross operation reservations",
            ));
        }
        let mut state = self.lock_state()?;
        let record = validate_record(&state, &binding, self.process_generation())?;
        if !matches!(record.value, CapabilityValue::Secret { .. }) {
            return Err(capability_error("archive secret has the wrong state"));
        }
        let record = remove_record(&mut state, binding.id)
            .ok_or_else(|| capability_error("unknown archive secret"))?;
        handle.0.disarm();
        match record.value {
            CapabilityValue::Secret { kind, mut bytes } => match kind {
                ArchiveCredentialKind::Password => {
                    ArchiveCredential::password(std::mem::take(&mut *bytes))
                        .map_err(NodeError::from)
                }
                ArchiveCredentialKind::RecoveryKey => {
                    let key: [u8; 32] = bytes
                        .as_slice()
                        .try_into()
                        .map_err(|_| capability_error("invalid recovery key"))?;
                    Ok(ArchiveCredential::RecoveryKey(RecoveryKey::from_bytes(
                        key,
                    )?))
                }
            },
            _ => Err(capability_error("archive secret has the wrong state")),
        }
    }

    pub(crate) fn publish_sink(
        &self,
        mut handle: WritableArchiveSinkHandle,
        expected_reservation: ArchiveOperationReservationId,
        bytes: Vec<u8>,
    ) -> Result<ReadableArchiveSinkHandle, NodeError> {
        let binding = handle.0.binding;
        if binding.reservation != expected_reservation {
            return Err(capability_error(
                "archive capabilities cross operation reservations",
            ));
        }
        let mut state = self.lock_state()?;
        let record = validate_record_mut(&mut state, &binding, self.process_generation())?;
        let maximum_bytes = match record.value {
            CapabilityValue::SinkWriting { maximum_bytes } => maximum_bytes,
            _ => return Err(capability_error("archive sink has the wrong state")),
        };
        if bytes.is_empty() || bytes.len() as u64 > maximum_bytes {
            remove_record(&mut state, binding.id);
            handle.0.disarm();
            return Err(capability_error("encrypted archive exceeds the sink bound"));
        }
        record.value = CapabilityValue::SinkReadable {
            maximum_bytes,
            bytes,
        };
        handle.0.disarm();
        Ok(ReadableArchiveSinkHandle(CapabilityHandle::new(
            binding,
            &self.inner,
        )))
    }

    pub(crate) fn discard_writable_sink(&self, mut handle: WritableArchiveSinkHandle) {
        self.inner.remove_best_effort(handle.id());
        handle.0.disarm();
    }

    fn insert_capability(
        &self,
        reservation: ArchiveOperationReservationId,
        value: CapabilityValue,
    ) -> Result<CapabilityBinding, NodeError> {
        let mut state = self.lock_state()?;
        if !state.reservations.contains(&reservation) {
            return Err(capability_error("archive reservation does not exist"));
        }
        if state.capabilities.len() >= MAX_ARCHIVE_CAPABILITIES {
            return Err(capability_error("archive capability limit reached"));
        }
        let reserved = value.reserved_bytes();
        let next_reserved = state
            .reserved_spool_bytes
            .checked_add(reserved)
            .ok_or_else(|| capability_error("archive spool accounting overflow"))?;
        if next_reserved > self.inner.max_spool_bytes {
            return Err(capability_error("archive spool quota exceeded"));
        }
        for _ in 0..32 {
            let id = ArchiveCapabilityId(random_id()?);
            if !state.capabilities.contains_key(&id) {
                let binding = CapabilityBinding {
                    id,
                    reservation,
                    process_generation: self.process_generation(),
                };
                state
                    .capabilities
                    .insert(id, CapabilityRecord { binding, value });
                state.reserved_spool_bytes = next_reserved;
                return Ok(binding);
            }
        }
        Err(capability_error("archive capability ID collision"))
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, RegistryState>, NodeError> {
        self.inner
            .state
            .lock()
            .map_err(|_| capability_error("archive capability registry is poisoned"))
    }
}

fn validate_record<'a>(
    state: &'a RegistryState,
    binding: &CapabilityBinding,
    generation: ArchiveProcessGeneration,
) -> Result<&'a CapabilityRecord, NodeError> {
    let record = state
        .capabilities
        .get(&binding.id)
        .ok_or_else(|| capability_error("unknown archive capability"))?;
    if record.binding.reservation != binding.reservation
        || record.binding.process_generation != generation
        || binding.process_generation != generation
    {
        return Err(capability_error("stale or forged archive capability"));
    }
    Ok(record)
}

fn validate_record_mut<'a>(
    state: &'a mut RegistryState,
    binding: &CapabilityBinding,
    generation: ArchiveProcessGeneration,
) -> Result<&'a mut CapabilityRecord, NodeError> {
    let record = state
        .capabilities
        .get_mut(&binding.id)
        .ok_or_else(|| capability_error("unknown archive capability"))?;
    if record.binding.reservation != binding.reservation
        || record.binding.process_generation != generation
        || binding.process_generation != generation
    {
        return Err(capability_error("stale or forged archive capability"));
    }
    Ok(record)
}

fn remove_record(state: &mut RegistryState, id: ArchiveCapabilityId) -> Option<CapabilityRecord> {
    let record = state.capabilities.remove(&id)?;
    state.reserved_spool_bytes = state
        .reserved_spool_bytes
        .saturating_sub(record.value.reserved_bytes());
    let reservation = record.binding.reservation;
    if !state
        .capabilities
        .values()
        .any(|remaining| remaining.binding.reservation == reservation)
    {
        state.reservations.remove(&reservation);
    }
    Some(record)
}

fn random_id() -> Result<[u8; 32], NodeError> {
    let mut id = [0u8; 32];
    getrandom::fill(&mut id).map_err(|_| capability_error("OS entropy is unavailable"))?;
    if id == [0; 32] {
        return Err(capability_error(
            "OS entropy returned an invalid identifier",
        ));
    }
    Ok(id)
}

fn capability_error(message: impl Into<String>) -> NodeError {
    NodeError::ArchiveCapability(message.into())
}
