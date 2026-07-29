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
const DRAFT_AAD_CONTEXT: &[u8] = b"onebrain:mobile:private-raw-draft:1\0";
const DRAFT_FORMAT_VERSION: u8 = 1;
const DRAFT_ID_BYTES: usize = 16;
const NONCE_BYTES: usize = 24;
const MAX_DRAFT_TEXT_BYTES: usize = 512 * 1024;
const MAX_LANGUAGE_TAG_BYTES: usize = 35;
const MAX_DRAFT_RECORDS: u64 = 10_000;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawDraftReceipt {
    pub draft_ref: String,
    pub content_language: String,
    pub content_bytes: u64,
    pub saved_at_monotonic_ms: u64,
    pub total_drafts: u64,
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

        let mut draft_id = [0u8; DRAFT_ID_BYTES];
        getrandom::fill(&mut draft_id)
            .map_err(|_| MobileCoreError::Security("draft CSPRNG unavailable".into()))?;
        let draft_ref = format!("draft_{}", hex::encode(draft_id));
        let record = RawDraftRecord {
            format_version: DRAFT_FORMAT_VERSION,
            draft_ref: draft_ref.clone(),
            content_language: content_language.to_ascii_lowercase(),
            saved_at_monotonic_ms,
            content_utf8: content_utf8.to_vec(),
        };
        let plaintext = Zeroizing::new(serde_json::to_vec(&record)?);
        let sealed = self.seal(&draft_ref, &plaintext)?;

        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(RAW_DRAFTS)?;
            if table.get(draft_ref.as_str())?.is_some() {
                return Err(MobileCoreError::Storage(
                    "generated raw draft reference collided".into(),
                ));
            }
            table.insert(draft_ref.as_str(), sealed.as_slice())?;
        }
        write.commit()?;
        Ok(RawDraftReceipt {
            draft_ref,
            content_language: record.content_language,
            content_bytes: u64::try_from(content_utf8.len()).unwrap_or(u64::MAX),
            saved_at_monotonic_ms,
            total_drafts: existing.saturating_add(1),
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
        let plaintext = Zeroizing::new(self.open_sealed(draft_ref, sealed.value())?);
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
        let mut plaintext = self.open_sealed(draft_ref, sealed.value())?;
        let record: RawDraftRecord = serde_json::from_slice(&plaintext)?;
        plaintext.fill(0);
        Ok(record.content_utf8)
    }

    fn seal(&self, draft_ref: &str, plaintext: &[u8]) -> Result<Vec<u8>, MobileCoreError> {
        let mut nonce_bytes = [0u8; NONCE_BYTES];
        getrandom::fill(&mut nonce_bytes)
            .map_err(|_| MobileCoreError::Security("draft nonce CSPRNG unavailable".into()))?;
        let nonce = XNonce::from(nonce_bytes);
        let aad = draft_aad(draft_ref);
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

    fn open_sealed(&self, draft_ref: &str, sealed: &[u8]) -> Result<Vec<u8>, MobileCoreError> {
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
                    aad: &draft_aad(draft_ref),
                },
            )
            .map_err(|_| {
                MobileCoreError::Security("private raw draft authentication failed".into())
            })
    }
}

fn draft_aad(draft_ref: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(DRAFT_AAD_CONTEXT.len() + draft_ref.len());
    aad.extend_from_slice(DRAFT_AAD_CONTEXT);
    aad.extend_from_slice(draft_ref.as_bytes());
    aad
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
}
