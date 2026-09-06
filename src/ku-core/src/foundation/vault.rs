//! Encrypted Private Vault over the shared validated/atomic storage abstraction.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use std::io::Write;
use std::path::{Path, PathBuf};
use zeroize::Zeroize;

use super::canonical::ResourceProfile;
use super::content_id::{EventCid, ObjectCid, ReservedDomain};
use super::event::{decode_knowledge_event, event_author_feed, EventType};
use super::feed::{decode_feed_inception, ValidatedFeedInception};
use super::object::{decode_knowledge_object, DisclosureClass, KnownObjectKind};
use super::source_text::{LocalSourceTextRecordV1, SourceTextError};
use super::storage::{AtomicVerifiedBackend, StoredRecordKind, ValidatedStore, VerifiedStoreError};
use super::{authority_event_descriptor, PutVerifiedOutcome};

pub struct VaultKey([u8; 32]);

impl VaultKey {
    /// The caller must obtain these bytes from a CSPRNG or an OS key store.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl Drop for VaultKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

struct VaultCipher {
    aead: XChaCha20Poly1305,
    nonce_key: [u8; 32],
}

impl VaultCipher {
    fn new(key: VaultKey) -> Self {
        let aead = XChaCha20Poly1305::new((&key.0).into());
        let nonce_key = blake3::derive_key("onebrain:vnext:vault-nonce-key:1", &key.0);
        Self { aead, nonce_key }
    }

    fn seal(
        &self,
        purpose: VaultPurpose,
        kind: StoredRecordKind,
        cid: [u8; 32],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, VerifiedStoreError> {
        let aad = vault_aad(purpose, kind, cid);
        let mut nonce_hasher = blake3::Hasher::new_keyed(&self.nonce_key);
        nonce_hasher.update(&aad);
        nonce_hasher.update(&blake3::hash(plaintext).as_bytes()[..]);
        let digest = nonce_hasher.finalize();
        let nonce = XNonce::try_from(&digest.as_bytes()[..24])
            .map_err(|_| VerifiedStoreError::VaultCrypto)?;
        let ciphertext = self
            .aead
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| VerifiedStoreError::VaultCrypto)?;
        let mut sealed = Vec::with_capacity(25 + ciphertext.len());
        sealed.push(1);
        sealed.extend_from_slice(&nonce);
        sealed.extend_from_slice(&ciphertext);
        Ok(sealed)
    }

    fn open(
        &self,
        purpose: VaultPurpose,
        kind: StoredRecordKind,
        cid: [u8; 32],
        sealed: &[u8],
    ) -> Result<Vec<u8>, VerifiedStoreError> {
        if sealed.len() < 41 || sealed[0] != 1 {
            return Err(VerifiedStoreError::VaultCrypto);
        }
        let aad = vault_aad(purpose, kind, cid);
        let nonce =
            XNonce::try_from(&sealed[1..25]).map_err(|_| VerifiedStoreError::VaultCrypto)?;
        self.aead
            .decrypt(
                &nonce,
                Payload {
                    msg: &sealed[25..],
                    aad: &aad,
                },
            )
            .map_err(|_| VerifiedStoreError::VaultCrypto)
    }
}

impl Drop for VaultCipher {
    fn drop(&mut self) {
        self.nonce_key.zeroize();
    }
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum VaultPurpose {
    Accepted = 1,
    Quarantine = 2,
    Staging = 3,
    LocalMetadata = 4,
}

fn vault_aad(purpose: VaultPurpose, kind: StoredRecordKind, cid: [u8; 32]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(69);
    aad.extend_from_slice(b"onebrain:vnext:private-vault:1\0");
    aad.push(purpose as u8);
    aad.push(kind as u8);
    aad.extend_from_slice(&cid);
    aad
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultQuarantineRecord {
    pub quarantine_id: [u8; 32],
    pub record_kind: StoredRecordKind,
    pub claimed_cid: [u8; 32],
    pub reason_code: String,
    pub plaintext: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VaultStagingId(pub [u8; 32]);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultSourceSnapshotRecord {
    pub subject: super::object::ObjectReference,
    pub source_record: ObjectCid,
    pub source_digest: [u8; 32],
    pub source_text: super::source_text::BoundedUtf8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortableVaultRecord {
    pub record_kind: StoredRecordKind,
    pub claimed_cid: [u8; 32],
    pub canonical_plaintext: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortableVaultSnapshot {
    pub accepted: Vec<PortableVaultRecord>,
    pub quarantine: Vec<VaultQuarantineRecord>,
}

pub trait PortableVaultSnapshotPort {
    fn portable_vault_snapshot(&self) -> Result<PortableVaultSnapshot, VerifiedStoreError>;
}

/// Restore implementations validate canonical plaintext and encrypt it under
/// the target Vault key; source database ciphertext is never accepted.
pub trait ValidatedVaultRestorePort {
    fn restore_vault_record(&self, record: &PortableVaultRecord) -> Result<(), VerifiedStoreError>;
    fn restore_vault_quarantine(
        &self,
        record: &VaultQuarantineRecord,
    ) -> Result<(), VerifiedStoreError>;
}

pub trait VaultSourceSnapshotPort {
    fn source_snapshot(&self) -> Result<Vec<VaultSourceSnapshotRecord>, VerifiedStoreError>;
    fn vault_source_root(&self) -> Result<[u8; 32], VerifiedStoreError>;
}

impl VaultQuarantineRecord {
    pub const fn is_executable(&self) -> bool {
        false
    }
}

/// Uses a dedicated backend/database. Both accepted payloads and quarantine
/// payloads are encrypted before reaching that backend.
pub struct PrivateVault<B> {
    store: ValidatedStore<B>,
    cipher: VaultCipher,
}

impl<B: AtomicVerifiedBackend> PrivateVault<B> {
    /// Node-owned encrypted local journals, with a purpose distinct from accepted
    /// canonical data. This never grants an accepted-object write capability.
    pub fn seal_local_metadata(
        &self,
        binding: [u8; 32],
        bytes: &[u8],
    ) -> Result<Vec<u8>, VerifiedStoreError> {
        if bytes.len() > 8 * 1024 * 1024 {
            return Err(VerifiedStoreError::VaultCrypto);
        }
        self.cipher.seal(
            VaultPurpose::LocalMetadata,
            StoredRecordKind::Object,
            binding,
            bytes,
        )
    }

    pub fn open_local_metadata(
        &self,
        binding: [u8; 32],
        sealed: &[u8],
    ) -> Result<Vec<u8>, VerifiedStoreError> {
        if sealed.len() > 8 * 1024 * 1024 + 41 {
            return Err(VerifiedStoreError::VaultCrypto);
        }
        self.cipher.open(
            VaultPurpose::LocalMetadata,
            StoredRecordKind::Object,
            binding,
            sealed,
        )
    }

    pub fn new(backend: B, key: VaultKey) -> Self {
        Self {
            store: ValidatedStore::new(backend),
            cipher: VaultCipher::new(key),
        }
    }

    pub fn put_verified_object(
        &self,
        claimed_cid: ObjectCid,
        bytes: &[u8],
        profile: ResourceProfile,
        known_kinds: &[KnownObjectKind],
        known_critical_extensions: &[u64],
    ) -> Result<PutVerifiedOutcome, VerifiedStoreError> {
        let cid = claimed_cid.into_bytes();
        let validated =
            match decode_knowledge_object(bytes, profile, known_kinds, known_critical_extensions) {
                Ok(validated) => validated,
                Err(error) => {
                    return self.quarantine(StoredRecordKind::Object, cid, error.code(), bytes)
                }
            };
        if !matches!(
            validated.disclosure(),
            DisclosureClass::NegotiatedEncrypted | DisclosureClass::LocalOnly
        ) {
            return Err(VerifiedStoreError::StorageClassMismatch(
                validated.disclosure(),
            ));
        }
        if validated.cid() != claimed_cid {
            return self.quarantine(StoredRecordKind::Object, cid, "CID_MISMATCH", bytes);
        }
        let sealed = self.cipher.seal(
            VaultPurpose::Accepted,
            StoredRecordKind::Object,
            cid,
            validated.original_bytes(),
        )?;
        self.store.accept(StoredRecordKind::Object, cid, &sealed)
    }

    pub fn put_verified_event(
        &self,
        claimed_cid: EventCid,
        bytes: &[u8],
        author: &ValidatedFeedInception,
        known_event_types: &[EventType],
    ) -> Result<PutVerifiedOutcome, VerifiedStoreError> {
        let cid = claimed_cid.into_bytes();
        let validated = match decode_knowledge_event(bytes, author, known_event_types) {
            Ok(validated) => validated,
            Err(error) => {
                return self.quarantine(StoredRecordKind::Event, cid, error.code(), bytes)
            }
        };
        if !matches!(
            validated.signed.event.disclosure,
            DisclosureClass::NegotiatedEncrypted | DisclosureClass::LocalOnly
        ) {
            return Err(VerifiedStoreError::StorageClassMismatch(
                validated.signed.event.disclosure,
            ));
        }
        if validated.cid() != claimed_cid {
            return self.quarantine(StoredRecordKind::Event, cid, "CID_MISMATCH", bytes);
        }
        let sealed = self.cipher.seal(
            VaultPurpose::Accepted,
            StoredRecordKind::Event,
            cid,
            validated.original_bytes(),
        )?;
        self.store.accept(StoredRecordKind::Event, cid, &sealed)
    }

    pub fn get_object(&self, cid: ObjectCid) -> Result<Option<Vec<u8>>, VerifiedStoreError> {
        let bytes = self.get(StoredRecordKind::Object, cid.into_bytes())?;
        verify_vault_cid(bytes, ReservedDomain::Object, cid.as_bytes())
    }

    pub fn get_event(&self, cid: EventCid) -> Result<Option<Vec<u8>>, VerifiedStoreError> {
        let bytes = self.get(StoredRecordKind::Event, cid.into_bytes())?;
        verify_vault_cid(bytes, ReservedDomain::Event, cid.as_bytes())
    }

    pub fn put_source_text(
        &self,
        record: &LocalSourceTextRecordV1,
    ) -> Result<(ObjectCid, PutVerifiedOutcome), VerifiedStoreError> {
        let (bytes, cid) = record.encode().map_err(source_text_store_error)?;
        let sealed = self.cipher.seal(
            VaultPurpose::Accepted,
            StoredRecordKind::Object,
            cid.into_bytes(),
            &bytes,
        )?;
        let outcome = self
            .store
            .accept(StoredRecordKind::Object, cid.into_bytes(), &sealed)?;
        Ok((cid, outcome))
    }

    pub fn get_source_text(
        &self,
        cid: ObjectCid,
    ) -> Result<Option<LocalSourceTextRecordV1>, VerifiedStoreError> {
        self.get(StoredRecordKind::Object, cid.into_bytes())?
            .map(|bytes| {
                let bytes = zeroize::Zeroizing::new(bytes);
                LocalSourceTextRecordV1::decode(&bytes).map_err(source_text_store_error)
            })
            .transpose()
    }

    /// Encrypt exact source bytes before they enter the durable staging area.
    /// The caller persists only authenticated metadata and this opaque ID.
    pub fn stage_source_text(
        &self,
        staging_root: &Path,
        staging_id: VaultStagingId,
        record: &LocalSourceTextRecordV1,
    ) -> Result<ObjectCid, VerifiedStoreError> {
        let (bytes, cid) = record.encode().map_err(source_text_store_error)?;
        let sealed = self.cipher.seal(
            VaultPurpose::Staging,
            StoredRecordKind::Object,
            staging_id.0,
            &bytes,
        )?;
        std::fs::create_dir_all(staging_root)
            .map_err(|error| VerifiedStoreError::Backend(error.to_string()))?;
        write_create_new_synced(&staging_path(staging_root, staging_id), &sealed)?;
        sync_directory(staging_root)?;
        Ok(cid)
    }

    pub fn inspect_staged_source(
        &self,
        staging_root: &Path,
        staging_id: VaultStagingId,
    ) -> Result<LocalSourceTextRecordV1, VerifiedStoreError> {
        let sealed = std::fs::read(staging_path(staging_root, staging_id))
            .map_err(|error| VerifiedStoreError::Backend(error.to_string()))?;
        let bytes = zeroize::Zeroizing::new(self.cipher.open(
            VaultPurpose::Staging,
            StoredRecordKind::Object,
            staging_id.0,
            &sealed,
        )?);
        LocalSourceTextRecordV1::decode(&bytes).map_err(source_text_store_error)
    }

    pub fn bind_staged_source(
        &self,
        staging_root: &Path,
        staging_id: VaultStagingId,
        expected_record: ObjectCid,
    ) -> Result<PutVerifiedOutcome, VerifiedStoreError> {
        let record = self.inspect_staged_source(staging_root, staging_id)?;
        let (bytes, cid) = record.encode().map_err(source_text_store_error)?;
        if cid != expected_record {
            return Err(VerifiedStoreError::VaultCidMismatch);
        }
        let sealed = self.cipher.seal(
            VaultPurpose::Accepted,
            StoredRecordKind::Object,
            cid.into_bytes(),
            &bytes,
        )?;
        let outcome = self
            .store
            .accept(StoredRecordKind::Object, cid.into_bytes(), &sealed)?;
        std::fs::remove_file(staging_path(staging_root, staging_id))
            .map_err(|error| VerifiedStoreError::Backend(error.to_string()))?;
        sync_directory(staging_root)?;
        Ok(outcome)
    }

    pub fn quarantine_staged_source(
        &self,
        staging_root: &Path,
        staging_id: VaultStagingId,
        reason: &str,
    ) -> Result<(), VerifiedStoreError> {
        let record = self.inspect_staged_source(staging_root, staging_id)?;
        let (bytes, cid) = record.encode().map_err(source_text_store_error)?;
        self.quarantine(StoredRecordKind::Object, cid.into_bytes(), reason, &bytes)?;
        std::fs::remove_file(staging_path(staging_root, staging_id))
            .map_err(|error| VerifiedStoreError::Backend(error.to_string()))?;
        sync_directory(staging_root)
    }

    pub fn source_intent_auth_tag(&self, metadata: &[u8]) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new_keyed(&self.cipher.nonce_key);
        hasher.update(b"onebrain:vnext:source-capture-intent:1\0");
        hasher.update(metadata);
        *hasher.finalize().as_bytes()
    }

    pub fn staged_source_exists(&self, staging_root: &Path, staging_id: VaultStagingId) -> bool {
        staging_path(staging_root, staging_id).is_file()
    }

    pub fn get_quarantine(
        &self,
        quarantine_id: &[u8; 32],
    ) -> Result<Option<VaultQuarantineRecord>, VerifiedStoreError> {
        let Some(record) = self.store.get_quarantine(quarantine_id)? else {
            return Ok(None);
        };
        let plaintext = self.cipher.open(
            VaultPurpose::Quarantine,
            record.record_kind,
            record.claimed_cid,
            &record.original_bytes,
        )?;
        Ok(Some(VaultQuarantineRecord {
            quarantine_id: record.quarantine_id,
            record_kind: record.record_kind,
            claimed_cid: record.claimed_cid,
            reason_code: record.reason_code,
            plaintext,
        }))
    }

    fn get(
        &self,
        kind: StoredRecordKind,
        cid: [u8; 32],
    ) -> Result<Option<Vec<u8>>, VerifiedStoreError> {
        let Some(sealed) = self.store.get(kind, cid)? else {
            return Ok(None);
        };
        self.cipher
            .open(VaultPurpose::Accepted, kind, cid, &sealed)
            .map(Some)
    }

    fn quarantine(
        &self,
        kind: StoredRecordKind,
        cid: [u8; 32],
        reason: &str,
        bytes: &[u8],
    ) -> Result<PutVerifiedOutcome, VerifiedStoreError> {
        let sealed = self
            .cipher
            .seal(VaultPurpose::Quarantine, kind, cid, bytes)?;
        self.store.quarantine(kind, cid, reason, &sealed)
    }

    #[cfg(test)]
    fn raw_accepted_for_test(
        &self,
        kind: StoredRecordKind,
        cid: [u8; 32],
    ) -> Result<Option<Vec<u8>>, VerifiedStoreError> {
        self.store.get(kind, cid)
    }
}

impl<B: AtomicVerifiedBackend> VaultSourceSnapshotPort for PrivateVault<B> {
    fn source_snapshot(&self) -> Result<Vec<VaultSourceSnapshotRecord>, VerifiedStoreError> {
        let mut records = Vec::new();
        for entry in self.store.accepted_entries(StoredRecordKind::Object)? {
            let plaintext = zeroize::Zeroizing::new(self.cipher.open(
                VaultPurpose::Accepted,
                StoredRecordKind::Object,
                entry.claimed_cid,
                &entry.canonical_bytes,
            )?);
            let record = match LocalSourceTextRecordV1::decode(&plaintext) {
                Ok(record) => record,
                Err(SourceTextError::NotSourceText) => continue,
                Err(error) => return Err(source_text_store_error(error)),
            };
            let source_record = ObjectCid::from_bytes(entry.claimed_cid);
            let (_, computed) = record.encode().map_err(source_text_store_error)?;
            if computed != source_record {
                return Err(VerifiedStoreError::VaultCidMismatch);
            }
            records.push(VaultSourceSnapshotRecord {
                subject: record.subject,
                source_record,
                source_digest: record.source_digest,
                source_text: record.source_text,
            });
        }
        records.sort_by_key(|record| {
            (
                record.subject.reference_kind,
                record.subject.cid,
                record.source_record.into_bytes(),
            )
        });
        Ok(records)
    }

    fn vault_source_root(&self) -> Result<[u8; 32], VerifiedStoreError> {
        let mut hasher = blake3::Hasher::new_derive_key("onebrain:vnext:vault-source-root:1");
        for record in self.source_snapshot()? {
            hasher.update(&record.subject.reference_kind.to_be_bytes());
            hasher.update(&record.subject.cid);
            hasher.update(record.source_record.as_bytes());
            hasher.update(&record.source_digest);
        }
        Ok(*hasher.finalize().as_bytes())
    }
}

impl<B: AtomicVerifiedBackend> PortableVaultSnapshotPort for PrivateVault<B> {
    fn portable_vault_snapshot(&self) -> Result<PortableVaultSnapshot, VerifiedStoreError> {
        let mut accepted = Vec::new();
        for kind in [
            StoredRecordKind::Object,
            StoredRecordKind::Event,
            StoredRecordKind::FeedInception,
            StoredRecordKind::AuthorityEvent,
        ] {
            for entry in self.store.accepted_entries(kind)? {
                let plaintext = zeroize::Zeroizing::new(self.cipher.open(
                    VaultPurpose::Accepted,
                    kind,
                    entry.claimed_cid,
                    &entry.canonical_bytes,
                )?);
                accepted.push(PortableVaultRecord {
                    record_kind: kind,
                    claimed_cid: entry.claimed_cid,
                    canonical_plaintext: plaintext.to_vec(),
                });
            }
        }
        accepted.sort_by_key(|record| (record.record_kind as u8, record.claimed_cid));

        let mut quarantine = Vec::new();
        for record in self.store.quarantine_entries()? {
            let plaintext = zeroize::Zeroizing::new(self.cipher.open(
                VaultPurpose::Quarantine,
                record.record_kind,
                record.claimed_cid,
                &record.original_bytes,
            )?);
            quarantine.push(VaultQuarantineRecord {
                quarantine_id: vault_portable_quarantine_id(
                    record.record_kind,
                    record.claimed_cid,
                    &record.reason_code,
                    &plaintext,
                ),
                record_kind: record.record_kind,
                claimed_cid: record.claimed_cid,
                reason_code: record.reason_code,
                plaintext: plaintext.to_vec(),
            });
        }
        quarantine.sort_by_key(|record| record.quarantine_id);
        Ok(PortableVaultSnapshot {
            accepted,
            quarantine,
        })
    }
}

impl<B: AtomicVerifiedBackend> ValidatedVaultRestorePort for PrivateVault<B> {
    fn restore_vault_record(&self, record: &PortableVaultRecord) -> Result<(), VerifiedStoreError> {
        validate_portable_vault_record(record)?;
        let sealed = self.cipher.seal(
            VaultPurpose::Accepted,
            record.record_kind,
            record.claimed_cid,
            &record.canonical_plaintext,
        )?;
        match self
            .store
            .accept(record.record_kind, record.claimed_cid, &sealed)?
        {
            PutVerifiedOutcome::Stored | PutVerifiedOutcome::AlreadyPresent => Ok(()),
            PutVerifiedOutcome::Quarantined { .. } => Err(VerifiedStoreError::Backend(
                "VAULT_RESTORE_ACCEPTED_CONFLICT".into(),
            )),
        }
    }

    fn restore_vault_quarantine(
        &self,
        record: &VaultQuarantineRecord,
    ) -> Result<(), VerifiedStoreError> {
        if record.reason_code.is_empty()
            || record.reason_code.len() > 128
            || record.quarantine_id
                != vault_portable_quarantine_id(
                    record.record_kind,
                    record.claimed_cid,
                    &record.reason_code,
                    &record.plaintext,
                )
        {
            return Err(VerifiedStoreError::Backend(
                "VAULT_RESTORE_QUARANTINE_IDENTITY".into(),
            ));
        }
        self.quarantine(
            record.record_kind,
            record.claimed_cid,
            &record.reason_code,
            &record.plaintext,
        )?;
        Ok(())
    }
}

fn validate_portable_vault_record(record: &PortableVaultRecord) -> Result<(), VerifiedStoreError> {
    let digest_matches = match record.record_kind {
        StoredRecordKind::Object => {
            let is_source_text = LocalSourceTextRecordV1::decode(&record.canonical_plaintext)
                .map(|source| {
                    source
                        .encode()
                        .map(|(bytes, cid)| {
                            bytes == record.canonical_plaintext
                                && cid.as_bytes() == &record.claimed_cid
                        })
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if is_source_text {
                true
            } else {
                let object = decode_knowledge_object(
                    &record.canonical_plaintext,
                    ResourceProfile::ObjectV1,
                    &[],
                    &[],
                )
                .map_err(|error| VerifiedStoreError::Backend(error.to_string()))?;
                matches!(
                    object.disclosure(),
                    DisclosureClass::NegotiatedEncrypted | DisclosureClass::LocalOnly
                ) && object.cid().as_bytes() == &record.claimed_cid
                    && object.original_bytes() == record.canonical_plaintext
            }
        }
        StoredRecordKind::Event => {
            event_author_feed(&record.canonical_plaintext)
                .map_err(|error| VerifiedStoreError::Backend(error.to_string()))?;
            EventCid::compute(ReservedDomain::Event, &record.canonical_plaintext)
                .map_err(|error| VerifiedStoreError::Backend(error.to_string()))?
                .as_bytes()
                == &record.claimed_cid
        }
        StoredRecordKind::FeedInception => {
            let feed = decode_feed_inception(&record.canonical_plaintext)
                .map_err(|error| VerifiedStoreError::Backend(error.to_string()))?;
            feed.feed_id.as_bytes() == &record.claimed_cid
                && feed.original_bytes() == record.canonical_plaintext
        }
        StoredRecordKind::AuthorityEvent => {
            authority_event_descriptor(&record.canonical_plaintext)
                .map_err(|error| VerifiedStoreError::Backend(error.to_string()))?;
            EventCid::compute(ReservedDomain::AuthorityEvent, &record.canonical_plaintext)
                .map_err(|error| VerifiedStoreError::Backend(error.to_string()))?
                .as_bytes()
                == &record.claimed_cid
        }
    };
    if digest_matches {
        Ok(())
    } else {
        Err(VerifiedStoreError::VaultCidMismatch)
    }
}

fn vault_portable_quarantine_id(
    kind: StoredRecordKind,
    claimed_cid: [u8; 32],
    reason: &str,
    plaintext: &[u8],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:vault-portable-quarantine:1\0");
    hasher.update(&[kind as u8]);
    hasher.update(&claimed_cid);
    hasher.update(&(reason.len() as u64).to_be_bytes());
    hasher.update(reason.as_bytes());
    hasher.update(plaintext);
    *hasher.finalize().as_bytes()
}

fn source_text_store_error(error: SourceTextError) -> VerifiedStoreError {
    VerifiedStoreError::Backend(error.to_string())
}

fn staging_path(root: &Path, staging_id: VaultStagingId) -> PathBuf {
    let mut name = String::with_capacity(70);
    for byte in staging_id.0 {
        use std::fmt::Write as _;
        let _ = write!(name, "{byte:02x}");
    }
    name.push_str(".stage");
    root.join(name)
}

fn write_create_new_synced(path: &Path, bytes: &[u8]) -> Result<(), VerifiedStoreError> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options
        .open(path)
        .map_err(|error| VerifiedStoreError::Backend(error.to_string()))?;
    file.write_all(bytes)
        .map_err(|error| VerifiedStoreError::Backend(error.to_string()))?;
    file.sync_all()
        .map_err(|error| VerifiedStoreError::Backend(error.to_string()))
}

fn sync_directory(directory: &Path) -> Result<(), VerifiedStoreError> {
    match std::fs::File::open(directory) {
        Ok(file) => file
            .sync_all()
            .map_err(|error| VerifiedStoreError::Backend(error.to_string())),
        Err(error) if cfg!(windows) && error.kind() == std::io::ErrorKind::PermissionDenied => {
            Ok(())
        }
        Err(error) => Err(VerifiedStoreError::Backend(error.to_string())),
    }
}

fn verify_vault_cid(
    bytes: Option<Vec<u8>>,
    domain: ReservedDomain,
    expected: &[u8; 32],
) -> Result<Option<Vec<u8>>, VerifiedStoreError> {
    let Some(bytes) = bytes else {
        return Ok(None);
    };
    if &domain.digest(&bytes) != expected {
        return Err(VerifiedStoreError::VaultCidMismatch);
    }
    Ok(Some(bytes))
}

#[cfg(test)]
mod tests {
    #[test]
    fn local_metadata_is_bound_private_and_separate_from_accepted_records() {
        use super::*;
        use crate::foundation::InMemoryVerifiedBackend;
        let vault = PrivateVault::new(
            InMemoryVerifiedBackend::default(),
            VaultKey::from_bytes([17; 32]),
        );
        let binding = [18; 32];
        let raw = b"private operation provenance";
        let sealed = vault.seal_local_metadata(binding, raw).unwrap();
        assert_eq!(vault.seal_local_metadata(binding, raw).unwrap(), sealed);
        assert_eq!(vault.open_local_metadata(binding, &sealed).unwrap(), raw);
        assert!(!sealed.windows(raw.len()).any(|w| w == raw));
        assert!(vault.open_local_metadata([19; 32], &sealed).is_err());
        let mut changed = sealed.clone();
        changed[25] ^= 1;
        assert!(vault.open_local_metadata(binding, &changed).is_err());
        assert!(vault
            .cipher
            .open(
                VaultPurpose::Accepted,
                StoredRecordKind::Object,
                binding,
                &sealed
            )
            .is_err());
        assert!(vault
            .get_object(ObjectCid::from_bytes(binding))
            .unwrap()
            .is_none());
        assert!(vault
            .seal_local_metadata(binding, &vec![0; 8 * 1024 * 1024 + 1])
            .is_err());
    }
    use super::*;
    use crate::foundation::{
        CanonicalValue, InMemoryVerifiedBackend, KnowledgeObjectEnvelope, ObjectKind, SchemaVersion,
    };

    const KNOWN_KIND: KnownObjectKind = KnownObjectKind::new(ObjectKind(10), 1);

    fn private_object() -> (Vec<u8>, ObjectCid) {
        KnowledgeObjectEnvelope::new(
            ObjectKind(10),
            SchemaVersion::new(1, 0),
            DisclosureClass::LocalOnly,
            CanonicalValue::Map(vec![(0, CanonicalValue::Text("private phrase".into()))]),
        )
        .encode(ResourceProfile::ObjectV1)
        .unwrap()
    }

    #[test]
    fn private_plaintext_never_reaches_the_backend() {
        let vault = PrivateVault::new(
            InMemoryVerifiedBackend::default(),
            VaultKey::from_bytes([9; 32]),
        );
        let (bytes, cid) = private_object();
        assert_eq!(
            vault
                .put_verified_object(cid, &bytes, ResourceProfile::ObjectV1, &[KNOWN_KIND], &[])
                .unwrap(),
            PutVerifiedOutcome::Stored
        );
        let sealed = vault
            .raw_accepted_for_test(StoredRecordKind::Object, cid.into_bytes())
            .unwrap()
            .unwrap();
        assert!(!sealed.windows(bytes.len()).any(|window| window == bytes));
        assert_eq!(vault.get_object(cid).unwrap().unwrap(), bytes);
    }

    #[test]
    fn private_insert_is_deterministically_idempotent() {
        let vault = PrivateVault::new(
            InMemoryVerifiedBackend::default(),
            VaultKey::from_bytes([9; 32]),
        );
        let (bytes, cid) = private_object();
        vault
            .put_verified_object(cid, &bytes, ResourceProfile::ObjectV1, &[KNOWN_KIND], &[])
            .unwrap();
        assert_eq!(
            vault
                .put_verified_object(cid, &bytes, ResourceProfile::ObjectV1, &[KNOWN_KIND], &[])
                .unwrap(),
            PutVerifiedOutcome::AlreadyPresent
        );
    }

    #[test]
    fn private_quarantine_payload_is_also_encrypted_at_rest() {
        let vault = PrivateVault::new(
            InMemoryVerifiedBackend::default(),
            VaultKey::from_bytes([9; 32]),
        );
        let claimed = ObjectCid::from_bytes([4; 32]);
        let private_bytes = b"private malformed payload";
        let PutVerifiedOutcome::Quarantined { quarantine_id } = vault
            .put_verified_object(
                claimed,
                private_bytes,
                ResourceProfile::ObjectV1,
                &[KNOWN_KIND],
                &[],
            )
            .unwrap()
        else {
            panic!("malformed private input must be quarantined");
        };
        let raw = vault.store.get_quarantine(&quarantine_id).unwrap().unwrap();
        assert!(!raw
            .original_bytes
            .windows(private_bytes.len())
            .any(|window| window == private_bytes));
        let opened = vault.get_quarantine(&quarantine_id).unwrap().unwrap();
        assert_eq!(opened.plaintext, private_bytes);
        assert!(!opened.is_executable());
    }

    #[test]
    fn wrong_key_or_changed_ciphertext_is_rejected() {
        let cipher = VaultCipher::new(VaultKey::from_bytes([1; 32]));
        let wrong = VaultCipher::new(VaultKey::from_bytes([2; 32]));
        let cid = [3; 32];
        let mut sealed = cipher
            .seal(
                VaultPurpose::Accepted,
                StoredRecordKind::Object,
                cid,
                b"secret",
            )
            .unwrap();
        assert!(wrong
            .open(
                VaultPurpose::Accepted,
                StoredRecordKind::Object,
                cid,
                &sealed,
            )
            .is_err());
        let last = sealed.len() - 1;
        sealed[last] ^= 1;
        assert!(cipher
            .open(
                VaultPurpose::Accepted,
                StoredRecordKind::Object,
                cid,
                &sealed,
            )
            .is_err());
    }

    #[test]
    fn distinct_plaintexts_never_reuse_a_nonce_for_the_same_claimed_cid() {
        let cipher = VaultCipher::new(VaultKey::from_bytes([1; 32]));
        let cid = [3; 32];
        let left = cipher
            .seal(
                VaultPurpose::Accepted,
                StoredRecordKind::Object,
                cid,
                b"left",
            )
            .unwrap();
        let right = cipher
            .seal(
                VaultPurpose::Accepted,
                StoredRecordKind::Object,
                cid,
                b"right",
            )
            .unwrap();
        assert_ne!(&left[1..25], &right[1..25]);
    }

    #[test]
    fn portable_snapshot_validates_and_reencrypts_under_the_target_key() {
        let source = PrivateVault::new(
            InMemoryVerifiedBackend::default(),
            VaultKey::from_bytes([1; 32]),
        );
        let (bytes, cid) = private_object();
        source
            .put_verified_object(cid, &bytes, ResourceProfile::ObjectV1, &[KNOWN_KIND], &[])
            .unwrap();
        source
            .put_verified_object(
                ObjectCid::from_bytes([7; 32]),
                b"private malformed payload",
                ResourceProfile::ObjectV1,
                &[KNOWN_KIND],
                &[],
            )
            .unwrap();
        let snapshot = source.portable_vault_snapshot().unwrap();

        let target = PrivateVault::new(
            InMemoryVerifiedBackend::default(),
            VaultKey::from_bytes([2; 32]),
        );
        for record in &snapshot.accepted {
            target.restore_vault_record(record).unwrap();
        }
        for record in &snapshot.quarantine {
            target.restore_vault_quarantine(record).unwrap();
        }
        assert_eq!(target.portable_vault_snapshot().unwrap(), snapshot);

        let mut corrupt = snapshot.accepted[0].clone();
        corrupt.canonical_plaintext[0] ^= 1;
        assert!(target.restore_vault_record(&corrupt).is_err());
    }
}
