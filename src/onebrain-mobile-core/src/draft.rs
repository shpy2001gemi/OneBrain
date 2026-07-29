use std::{fmt, fs, path::Path};

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::MobileCoreError;

const RAW_DRAFTS: TableDefinition<&str, &[u8]> = TableDefinition::new("private_raw_drafts");
const SHARE_SPOOLS: TableDefinition<&str, &[u8]> = TableDefinition::new("private_share_spools");
const SHARE_CALLBACKS: TableDefinition<&str, &str> =
    TableDefinition::new("private_share_callbacks");
const DRAFT_AAD_CONTEXT: &[u8] = b"onebrain:mobile:private-raw-draft:1\0";
const SHARE_SPOOL_AAD_CONTEXT: &[u8] = b"onebrain:mobile:private-share-spool:1\0";
const DRAFT_FORMAT_VERSION: u8 = 1;
const SHARE_SPOOL_FORMAT_VERSION: u8 = 1;
const DRAFT_ID_BYTES: usize = 16;
const NONCE_BYTES: usize = 24;
const MAX_DRAFT_TEXT_BYTES: usize = 512 * 1024;
const MAX_LANGUAGE_TAG_BYTES: usize = 35;
const MAX_DRAFT_RECORDS: u64 = 10_000;
const MAX_SHARE_SPOOL_RECORDS: u64 = 10_000;
const MAX_PENDING_SHARE_SPOOLS: u64 = 64;
const MAX_CALLBACK_TOKEN_BYTES: usize = 96;
const MAX_MIME_TYPE_BYTES: usize = 63;
const MAX_SHARE_LIST_ITEMS: usize = 64;

pub struct PrivateDraftKey(Zeroizing<[u8; 32]>);

impl PrivateDraftKey {
    pub fn derive(vault_key: &[u8; 32]) -> Self {
        Self(Zeroizing::new(blake3::derive_key(
            "onebrain:mobile:private-draft-key:1",
            vault_key,
        )))
    }
}

impl fmt::Debug for PrivateDraftKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PrivateDraftKey([REDACTED])")
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct RawDraftRecord {
    format_version: u8,
    draft_ref: String,
    content_language: String,
    saved_at_monotonic_ms: u64,
    content_utf8: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ShareSpoolRecord {
    format_version: u8,
    spool_ref: String,
    callback_token: String,
    mime_type: String,
    received_at_monotonic_ms: u64,
    content_utf8: Vec<u8>,
    imported_draft_ref: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawDraftReceipt {
    pub draft_ref: String,
    pub content_language: String,
    pub content_bytes: u64,
    pub saved_at_monotonic_ms: u64,
    pub total_drafts: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShareSpoolSummary {
    pub spool_ref: String,
    pub mime_type: String,
    pub content_bytes: u64,
    pub received_at_monotonic_ms: u64,
}

pub struct PrivateDraftStore {
    database: Database,
    cipher: XChaCha20Poly1305,
}

impl PrivateDraftStore {
    pub fn open(path: &Path, key: PrivateDraftKey) -> Result<Self, MobileCoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                MobileCoreError::Storage(format!(
                    "cannot create private draft directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        let database = Database::create(path)?;
        let write = database.begin_write()?;
        {
            write.open_table(RAW_DRAFTS)?;
            write.open_table(SHARE_SPOOLS)?;
            write.open_table(SHARE_CALLBACKS)?;
        }
        write.commit()?;
        let cipher = XChaCha20Poly1305::new((&*key.0).into());
        Ok(Self { database, cipher })
    }

    pub fn save_text(
        &self,
        content_language: &str,
        content_utf8: &[u8],
        saved_at_monotonic_ms: u64,
    ) -> Result<RawDraftReceipt, MobileCoreError> {
        validate_language_tag(content_language)?;
        if content_utf8.is_empty() || content_utf8.len() > MAX_DRAFT_TEXT_BYTES {
            return Err(MobileCoreError::InvalidArgument(format!(
                "raw draft text must contain 1..={MAX_DRAFT_TEXT_BYTES} UTF-8 bytes"
            )));
        }
        let text = std::str::from_utf8(content_utf8).map_err(|_| {
            MobileCoreError::InvalidArgument("raw draft text must be valid UTF-8".into())
        })?;
        if text.trim().is_empty() {
            return Err(MobileCoreError::InvalidArgument(
                "raw draft text cannot be blank".into(),
            ));
        }

        let existing = self.count()?;
        if existing >= MAX_DRAFT_RECORDS {
            return Err(MobileCoreError::Storage(
                "private raw draft record budget is exhausted".into(),
            ));
        }

        let (record, sealed) =
            self.prepare_raw_draft(content_language, content_utf8, saved_at_monotonic_ms)?;

        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(RAW_DRAFTS)?;
            if table.get(record.draft_ref.as_str())?.is_some() {
                return Err(MobileCoreError::Storage(
                    "generated raw draft reference collided".into(),
                ));
            }
            table.insert(record.draft_ref.as_str(), sealed.as_slice())?;
        }
        write.commit()?;
        Ok(RawDraftReceipt {
            draft_ref: record.draft_ref,
            content_language: record.content_language,
            content_bytes: u64::try_from(content_utf8.len()).unwrap_or(u64::MAX),
            saved_at_monotonic_ms,
            total_drafts: existing.saturating_add(1),
        })
    }

    pub fn enqueue_shared_text(
        &self,
        callback_token: &str,
        mime_type: &str,
        content_utf8: &[u8],
        received_at_monotonic_ms: u64,
    ) -> Result<ShareSpoolSummary, MobileCoreError> {
        validate_callback_token(callback_token)?;
        validate_mime_type(mime_type)?;
        validate_raw_text(content_utf8)?;

        let read = self.database.begin_read()?;
        let callbacks = read.open_table(SHARE_CALLBACKS)?;
        if let Some(existing_ref) = callbacks.get(callback_token)? {
            let summary = self
                .inspect_share_spool(existing_ref.value())?
                .ok_or_else(|| {
                    MobileCoreError::Security(
                        "share callback points to a missing encrypted spool".into(),
                    )
                })?;
            return Ok(summary);
        }
        drop(callbacks);
        drop(read);

        if self.share_spool_count()? >= MAX_SHARE_SPOOL_RECORDS
            || self.pending_share_spool_count()? >= MAX_PENDING_SHARE_SPOOLS
        {
            return Err(MobileCoreError::Storage(
                "private share spool record budget is exhausted".into(),
            ));
        }

        let spool_ref = random_opaque_ref("spool")?;
        let record = ShareSpoolRecord {
            format_version: SHARE_SPOOL_FORMAT_VERSION,
            spool_ref: spool_ref.clone(),
            callback_token: callback_token.to_owned(),
            mime_type: mime_type.to_ascii_lowercase(),
            received_at_monotonic_ms,
            content_utf8: content_utf8.to_vec(),
            imported_draft_ref: None,
        };
        let plaintext = Zeroizing::new(serde_json::to_vec(&record)?);
        let sealed = self.seal(
            SHARE_SPOOL_AAD_CONTEXT,
            record.spool_ref.as_str(),
            &plaintext,
        )?;
        let write = self.database.begin_write()?;
        {
            let mut spools = write.open_table(SHARE_SPOOLS)?;
            let mut callbacks = write.open_table(SHARE_CALLBACKS)?;
            if spools.get(record.spool_ref.as_str())?.is_some()
                || callbacks.get(callback_token)?.is_some()
            {
                return Err(MobileCoreError::Storage(
                    "share spool reference collided".into(),
                ));
            }
            spools.insert(record.spool_ref.as_str(), sealed.as_slice())?;
            callbacks.insert(callback_token, record.spool_ref.as_str())?;
        }
        write.commit()?;
        Ok(ShareSpoolSummary {
            spool_ref: record.spool_ref,
            mime_type: record.mime_type,
            content_bytes: u64::try_from(content_utf8.len()).unwrap_or(u64::MAX),
            received_at_monotonic_ms,
        })
    }

    pub fn pending_share_spools(
        &self,
        limit: usize,
    ) -> Result<Vec<ShareSpoolSummary>, MobileCoreError> {
        let bounded_limit = limit.min(MAX_SHARE_LIST_ITEMS);
        let read = self.database.begin_read()?;
        let table = read.open_table(SHARE_SPOOLS)?;
        let mut summaries = Vec::new();
        for entry in table.iter()? {
            let (reference, sealed) = entry?;
            let spool_ref = reference.value();
            let record = self.open_share_spool(spool_ref, sealed.value())?;
            if record.imported_draft_ref.is_none() {
                summaries.push(ShareSpoolSummary {
                    spool_ref: record.spool_ref,
                    mime_type: record.mime_type,
                    content_bytes: u64::try_from(record.content_utf8.len()).unwrap_or(u64::MAX),
                    received_at_monotonic_ms: record.received_at_monotonic_ms,
                });
            }
        }
        summaries.sort_by_key(|summary| summary.received_at_monotonic_ms);
        summaries.truncate(bounded_limit);
        Ok(summaries)
    }

    pub fn pending_share_spool_count(&self) -> Result<u64, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(SHARE_SPOOLS)?;
        let mut count = 0u64;
        for entry in table.iter()? {
            let (reference, sealed) = entry?;
            if self
                .open_share_spool(reference.value(), sealed.value())?
                .imported_draft_ref
                .is_none()
            {
                count = count.saturating_add(1);
            }
        }
        Ok(count)
    }

    pub fn inspect_share_spool(
        &self,
        spool_ref: &str,
    ) -> Result<Option<ShareSpoolSummary>, MobileCoreError> {
        validate_spool_ref(spool_ref)?;
        let read = self.database.begin_read()?;
        let table = read.open_table(SHARE_SPOOLS)?;
        let Some(sealed) = table.get(spool_ref)? else {
            return Ok(None);
        };
        let record = self.open_share_spool(spool_ref, sealed.value())?;
        Ok(Some(ShareSpoolSummary {
            spool_ref: record.spool_ref,
            mime_type: record.mime_type,
            content_bytes: u64::try_from(record.content_utf8.len()).unwrap_or(u64::MAX),
            received_at_monotonic_ms: record.received_at_monotonic_ms,
        }))
    }

    pub fn import_shared_text(
        &self,
        spool_ref: &str,
        content_language: &str,
        saved_at_monotonic_ms: u64,
    ) -> Result<RawDraftReceipt, MobileCoreError> {
        validate_spool_ref(spool_ref)?;
        validate_language_tag(content_language)?;
        let read = self.database.begin_read()?;
        let spools = read.open_table(SHARE_SPOOLS)?;
        let sealed = spools
            .get(spool_ref)?
            .ok_or_else(|| MobileCoreError::InvalidArgument("share spool does not exist".into()))?;
        let mut spool = self.open_share_spool(spool_ref, sealed.value())?;
        drop(sealed);
        drop(spools);
        drop(read);

        if let Some(draft_ref) = spool.imported_draft_ref.as_deref() {
            return self.inspect(draft_ref)?.ok_or_else(|| {
                MobileCoreError::Security("imported share spool lost its draft binding".into())
            });
        }
        if spool.mime_type != "text/plain" {
            return Err(MobileCoreError::InvalidArgument(
                "only text/plain share spools can enter Limited raw drafts".into(),
            ));
        }
        let existing_drafts = self.count()?;
        if existing_drafts >= MAX_DRAFT_RECORDS {
            return Err(MobileCoreError::Storage(
                "private raw draft record budget is exhausted".into(),
            ));
        }
        let (draft, sealed_draft) =
            self.prepare_raw_draft(content_language, &spool.content_utf8, saved_at_monotonic_ms)?;
        let content_bytes = u64::try_from(spool.content_utf8.len()).unwrap_or(u64::MAX);
        spool.content_utf8.fill(0);
        spool.content_utf8.clear();
        spool.imported_draft_ref = Some(draft.draft_ref.clone());
        let sealed_spool = {
            let plaintext = Zeroizing::new(serde_json::to_vec(&spool)?);
            self.seal(SHARE_SPOOL_AAD_CONTEXT, spool_ref, &plaintext)?
        };

        let write = self.database.begin_write()?;
        {
            let mut drafts = write.open_table(RAW_DRAFTS)?;
            let mut spools = write.open_table(SHARE_SPOOLS)?;
            if drafts.get(draft.draft_ref.as_str())?.is_some() {
                return Err(MobileCoreError::Storage(
                    "generated raw draft reference collided".into(),
                ));
            }
            drafts.insert(draft.draft_ref.as_str(), sealed_draft.as_slice())?;
            spools.insert(spool_ref, sealed_spool.as_slice())?;
        }
        write.commit()?;
        Ok(RawDraftReceipt {
            draft_ref: draft.draft_ref,
            content_language: draft.content_language,
            content_bytes,
            saved_at_monotonic_ms,
            total_drafts: existing_drafts.saturating_add(1),
        })
    }

    pub fn count(&self) -> Result<u64, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(RAW_DRAFTS)?;
        Ok(table.len()?)
    }

    pub fn inspect(&self, draft_ref: &str) -> Result<Option<RawDraftReceipt>, MobileCoreError> {
        validate_draft_ref(draft_ref)?;
        let read = self.database.begin_read()?;
        let table = read.open_table(RAW_DRAFTS)?;
        let Some(sealed) = table.get(draft_ref)? else {
            return Ok(None);
        };
        let plaintext =
            Zeroizing::new(self.open_sealed(DRAFT_AAD_CONTEXT, draft_ref, sealed.value())?);
        let record: RawDraftRecord = serde_json::from_slice(&plaintext)?;
        if record.format_version != DRAFT_FORMAT_VERSION || record.draft_ref != draft_ref {
            return Err(MobileCoreError::Security(
                "private raw draft binding is invalid".into(),
            ));
        }
        Ok(Some(RawDraftReceipt {
            draft_ref: record.draft_ref,
            content_language: record.content_language,
            content_bytes: u64::try_from(record.content_utf8.len()).unwrap_or(u64::MAX),
            saved_at_monotonic_ms: record.saved_at_monotonic_ms,
            total_drafts: table.len()?,
        }))
    }

    #[cfg(test)]
    fn read_text_for_test(&self, draft_ref: &str) -> Result<Vec<u8>, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(RAW_DRAFTS)?;
        let sealed = table
            .get(draft_ref)?
            .ok_or_else(|| MobileCoreError::Storage("test draft missing".into()))?;
        let mut plaintext = self.open_sealed(DRAFT_AAD_CONTEXT, draft_ref, sealed.value())?;
        let record: RawDraftRecord = serde_json::from_slice(&plaintext)?;
        plaintext.fill(0);
        Ok(record.content_utf8)
    }

    fn prepare_raw_draft(
        &self,
        content_language: &str,
        content_utf8: &[u8],
        saved_at_monotonic_ms: u64,
    ) -> Result<(RawDraftRecord, Vec<u8>), MobileCoreError> {
        validate_language_tag(content_language)?;
        validate_raw_text(content_utf8)?;
        let draft_ref = random_opaque_ref("draft")?;
        let record = RawDraftRecord {
            format_version: DRAFT_FORMAT_VERSION,
            draft_ref: draft_ref.clone(),
            content_language: content_language.to_ascii_lowercase(),
            saved_at_monotonic_ms,
            content_utf8: content_utf8.to_vec(),
        };
        let plaintext = Zeroizing::new(serde_json::to_vec(&record)?);
        let sealed = self.seal(DRAFT_AAD_CONTEXT, &draft_ref, &plaintext)?;
        Ok((record, sealed))
    }

    fn share_spool_count(&self) -> Result<u64, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(SHARE_SPOOLS)?;
        Ok(table.len()?)
    }

    fn open_share_spool(
        &self,
        spool_ref: &str,
        sealed: &[u8],
    ) -> Result<ShareSpoolRecord, MobileCoreError> {
        let plaintext =
            Zeroizing::new(self.open_sealed(SHARE_SPOOL_AAD_CONTEXT, spool_ref, sealed)?);
        let record: ShareSpoolRecord = serde_json::from_slice(&plaintext)?;
        if record.format_version != SHARE_SPOOL_FORMAT_VERSION || record.spool_ref != spool_ref {
            return Err(MobileCoreError::Security(
                "private share spool binding is invalid".into(),
            ));
        }
        Ok(record)
    }

    fn seal(
        &self,
        aad_context: &[u8],
        object_ref: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, MobileCoreError> {
        let mut nonce_bytes = [0u8; NONCE_BYTES];
        getrandom::fill(&mut nonce_bytes)
            .map_err(|_| MobileCoreError::Security("draft nonce CSPRNG unavailable".into()))?;
        let nonce = XNonce::from(nonce_bytes);
        let aad = object_aad(aad_context, object_ref);
        let ciphertext = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| MobileCoreError::Security("cannot seal private raw draft".into()))?;
        let mut sealed = Vec::with_capacity(1 + NONCE_BYTES + ciphertext.len());
        sealed.push(DRAFT_FORMAT_VERSION);
        sealed.extend_from_slice(&nonce_bytes);
        sealed.extend_from_slice(&ciphertext);
        Ok(sealed)
    }

    fn open_sealed(
        &self,
        aad_context: &[u8],
        object_ref: &str,
        sealed: &[u8],
    ) -> Result<Vec<u8>, MobileCoreError> {
        if sealed.len() < 1 + NONCE_BYTES + 16 || sealed[0] != DRAFT_FORMAT_VERSION {
            return Err(MobileCoreError::Security(
                "private raw draft envelope is invalid".into(),
            ));
        }
        let nonce = XNonce::try_from(&sealed[1..1 + NONCE_BYTES])
            .map_err(|_| MobileCoreError::Security("private draft nonce is invalid".into()))?;
        self.cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &sealed[1 + NONCE_BYTES..],
                    aad: &object_aad(aad_context, object_ref),
                },
            )
            .map_err(|_| {
                MobileCoreError::Security("private raw draft authentication failed".into())
            })
    }
}

fn object_aad(context: &[u8], object_ref: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(context.len() + object_ref.len());
    aad.extend_from_slice(context);
    aad.extend_from_slice(object_ref.as_bytes());
    aad
}

fn random_opaque_ref(prefix: &str) -> Result<String, MobileCoreError> {
    let mut id = [0u8; DRAFT_ID_BYTES];
    getrandom::fill(&mut id)
        .map_err(|_| MobileCoreError::Security("private object CSPRNG unavailable".into()))?;
    Ok(format!("{prefix}_{}", hex::encode(id)))
}

fn validate_draft_ref(draft_ref: &str) -> Result<(), MobileCoreError> {
    let Some(hex_part) = draft_ref.strip_prefix("draft_") else {
        return Err(MobileCoreError::InvalidArgument(
            "raw draft reference is invalid".into(),
        ));
    };
    if hex_part.len() != DRAFT_ID_BYTES * 2
        || !hex_part.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(MobileCoreError::InvalidArgument(
            "raw draft reference is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_spool_ref(spool_ref: &str) -> Result<(), MobileCoreError> {
    validate_opaque_ref(spool_ref, "spool")
}

fn validate_language_tag(tag: &str) -> Result<(), MobileCoreError> {
    if tag.len() < 2
        || tag.len() > MAX_LANGUAGE_TAG_BYTES
        || !tag
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(MobileCoreError::InvalidArgument(
            "content language must be a bounded BCP-47 style tag".into(),
        ));
    }
    Ok(())
}

fn validate_opaque_ref(reference: &str, prefix: &str) -> Result<(), MobileCoreError> {
    let expected = format!("{prefix}_");
    let Some(hex_part) = reference.strip_prefix(&expected) else {
        return Err(MobileCoreError::InvalidArgument(
            "private object reference is invalid".into(),
        ));
    };
    if hex_part.len() != DRAFT_ID_BYTES * 2
        || !hex_part.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(MobileCoreError::InvalidArgument(
            "private object reference is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_callback_token(token: &str) -> Result<(), MobileCoreError> {
    if token.len() < 8
        || token.len() > MAX_CALLBACK_TOKEN_BYTES
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(MobileCoreError::InvalidArgument(
            "share callback token is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_mime_type(mime_type: &str) -> Result<(), MobileCoreError> {
    if mime_type.len() > MAX_MIME_TYPE_BYTES || mime_type != "text/plain" || !mime_type.is_ascii() {
        return Err(MobileCoreError::InvalidArgument(
            "Limited share intake accepts only text/plain".into(),
        ));
    }
    Ok(())
}

fn validate_raw_text(content_utf8: &[u8]) -> Result<(), MobileCoreError> {
    if content_utf8.is_empty() || content_utf8.len() > MAX_DRAFT_TEXT_BYTES {
        return Err(MobileCoreError::InvalidArgument(format!(
            "raw text must contain 1..={MAX_DRAFT_TEXT_BYTES} UTF-8 bytes"
        )));
    }
    let text = std::str::from_utf8(content_utf8)
        .map_err(|_| MobileCoreError::InvalidArgument("raw text must be valid UTF-8".into()))?;
    if text.trim().is_empty() {
        return Err(MobileCoreError::InvalidArgument(
            "raw text cannot be blank".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn raw_draft_is_encrypted_bounded_and_reopenable() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("private-drafts.redb");
        let secret = b"A bright private idea that must never appear in redb";
        let receipt = {
            let store = PrivateDraftStore::open(&path, PrivateDraftKey::derive(&[7; 32])).unwrap();
            let receipt = store.save_text("vi", secret, 42).unwrap();
            assert_eq!(receipt.content_bytes, secret.len() as u64);
            assert_eq!(receipt.total_drafts, 1);
            assert_eq!(
                store.read_text_for_test(&receipt.draft_ref).unwrap(),
                secret
            );
            receipt
        };
        let raw_database = fs::read(&path).unwrap();
        assert!(!raw_database
            .windows(secret.len())
            .any(|window| window == secret));
        let reopened = PrivateDraftStore::open(&path, PrivateDraftKey::derive(&[7; 32])).unwrap();
        assert_eq!(
            reopened.inspect(&receipt.draft_ref).unwrap().unwrap(),
            receipt
        );
    }

    #[test]
    fn wrong_key_corruption_and_invalid_input_fail_closed() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("private-drafts.redb");
        let reference = {
            let store = PrivateDraftStore::open(&path, PrivateDraftKey::derive(&[1; 32])).unwrap();
            assert!(store.save_text("vi", b"   ", 1).is_err());
            assert!(store.save_text("not valid!", b"text", 1).is_err());
            store.save_text("en-US", b"private", 1).unwrap().draft_ref
        };
        let wrong = PrivateDraftStore::open(&path, PrivateDraftKey::derive(&[2; 32])).unwrap();
        assert!(wrong.inspect(&reference).is_err());
    }

    #[test]
    fn share_spool_is_encrypted_deduplicated_and_imported_atomically() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("private-drafts.redb");
        let secret = b"Shared private idea from another app";
        let (spool_ref, draft_ref) = {
            let store = PrivateDraftStore::open(&path, PrivateDraftKey::derive(&[9; 32])).unwrap();
            let first = store
                .enqueue_shared_text(
                    "android:550e8400-e29b-41d4-a716-446655440000",
                    "text/plain",
                    secret,
                    7,
                )
                .unwrap();
            let duplicate = store
                .enqueue_shared_text(
                    "android:550e8400-e29b-41d4-a716-446655440000",
                    "text/plain",
                    secret,
                    8,
                )
                .unwrap();
            assert_eq!(first.spool_ref, duplicate.spool_ref);
            assert_eq!(store.pending_share_spool_count().unwrap(), 1);
            assert_eq!(store.pending_share_spools(10).unwrap(), vec![first.clone()]);

            let imported = store.import_shared_text(&first.spool_ref, "vi", 9).unwrap();
            let retry = store
                .import_shared_text(&first.spool_ref, "vi", 10)
                .unwrap();
            assert_eq!(retry.draft_ref, imported.draft_ref);
            assert_eq!(store.pending_share_spool_count().unwrap(), 0);
            assert_eq!(
                store.read_text_for_test(&imported.draft_ref).unwrap(),
                secret
            );
            (first.spool_ref, imported.draft_ref)
        };

        let bytes = fs::read(&path).unwrap();
        assert!(!bytes.windows(secret.len()).any(|window| window == secret));
        let reopened = PrivateDraftStore::open(&path, PrivateDraftKey::derive(&[9; 32])).unwrap();
        assert_eq!(reopened.pending_share_spool_count().unwrap(), 0);
        assert!(reopened.inspect_share_spool(&spool_ref).unwrap().is_some());
        assert_eq!(
            reopened.inspect(&draft_ref).unwrap().unwrap().content_bytes,
            secret.len() as u64
        );
    }

    #[test]
    fn share_spool_rejects_unbounded_or_unsupported_input() {
        let directory = tempdir().unwrap();
        let store = PrivateDraftStore::open(
            &directory.path().join("private-drafts.redb"),
            PrivateDraftKey::derive(&[3; 32]),
        )
        .unwrap();
        assert!(store
            .enqueue_shared_text("short", "text/plain", b"text", 1)
            .is_err());
        assert!(store
            .enqueue_shared_text(
                "android:550e8400-e29b-41d4-a716-446655440000",
                "text/html",
                b"<b>unsafe</b>",
                1,
            )
            .is_err());
        assert!(store
            .enqueue_shared_text(
                "android:550e8400-e29b-41d4-a716-446655440000",
                "text/plain",
                b"   ",
                1,
            )
            .is_err());
    }
}
