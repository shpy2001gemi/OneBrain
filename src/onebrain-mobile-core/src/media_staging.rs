use std::{
    fmt,
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::MobileCoreError;

const MEDIA_STAGES: TableDefinition<&str, &[u8]> = TableDefinition::new("private_media_stages");
const METADATA_AAD_CONTEXT: &[u8] = b"onebrain:mobile:private-media-stage-metadata:1\0";
const CHUNK_AAD_CONTEXT: &[u8] = b"onebrain:mobile:private-media-stage-chunk:1\0";
const FILE_MAGIC: &[u8; 8] = b"OBMSTG1\0";
const FORMAT_VERSION: u8 = 1;
const NONCE_BYTES: usize = 24;
const SOURCE_ID_BYTES: usize = 16;
const MAX_CHUNK_BYTES: usize = 256 * 1024;
const MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_ACTIVE_STAGES: usize = 4;
const MAX_DECLARED_MIME_BYTES: usize = 127;
const SNIFF_PREFIX_BYTES: usize = 32 * 1024;

pub struct MediaStagingKey(Zeroizing<[u8; 32]>);

impl MediaStagingKey {
    pub fn derive(vault_key: &[u8; 32]) -> Self {
        Self(Zeroizing::new(blake3::derive_key(
            "onebrain:mobile:private-media-staging-key:1",
            vault_key,
        )))
    }
}

impl fmt::Debug for MediaStagingKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MediaStagingKey([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaStageState {
    Receiving,
    StagedVerified,
    Interrupted,
    Rejected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct MediaStageRecord {
    format_version: u8,
    source_ref: String,
    requested_class: String,
    declared_mime_type: String,
    state: MediaStageState,
    created_at_monotonic_ms: u64,
    committed_chunks: u64,
    committed_plaintext_bytes: u64,
    committed_file_bytes: u64,
    detected_mime_type: Option<String>,
    blake3_digest: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaStageReceipt {
    pub source_ref: String,
    pub media_class: String,
    pub mime_type: String,
    pub content_bytes: u64,
    pub blake3_digest: String,
    pub state: MediaStageState,
}

pub struct MediaStagingStore {
    database: Database,
    root: PathBuf,
    cipher: XChaCha20Poly1305,
}

impl MediaStagingStore {
    pub fn open(
        database_path: &Path,
        root: &Path,
        key: MediaStagingKey,
    ) -> Result<Self, MobileCoreError> {
        if let Some(parent) = database_path.parent() {
            create_dir_all(parent)?;
        }
        create_dir_all(root)?;
        let database = Database::create(database_path)?;
        let write = database.begin_write()?;
        {
            write.open_table(MEDIA_STAGES)?;
        }
        write.commit()?;
        let store = Self {
            database,
            root: root.to_path_buf(),
            cipher: XChaCha20Poly1305::new((&*key.0).into()),
        };
        store.recover_interrupted_stages()?;
        Ok(store)
    }

    pub fn start(
        &self,
        requested_class: &str,
        declared_mime_type: &str,
        created_at_monotonic_ms: u64,
    ) -> Result<String, MobileCoreError> {
        validate_requested_class(requested_class)?;
        validate_declared_mime(declared_mime_type)?;
        if self.receiving_count()? >= MAX_ACTIVE_STAGES {
            return Err(MobileCoreError::BudgetExceeded(
                "too many private media staging operations".into(),
            ));
        }

        let source_ref = random_source_ref()?;
        let path = self.stage_path(&source_ref);
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|error| storage_error("cannot create encrypted media stage", error))?;
        file.write_all(FILE_MAGIC)
            .map_err(|error| storage_error("cannot write media stage header", error))?;
        file.sync_all()
            .map_err(|error| storage_error("cannot sync media stage header", error))?;

        let record = MediaStageRecord {
            format_version: FORMAT_VERSION,
            source_ref: source_ref.clone(),
            requested_class: requested_class.to_ascii_lowercase(),
            declared_mime_type: declared_mime_type.to_ascii_lowercase(),
            state: MediaStageState::Receiving,
            created_at_monotonic_ms,
            committed_chunks: 0,
            committed_plaintext_bytes: 0,
            committed_file_bytes: FILE_MAGIC.len() as u64,
            detected_mime_type: None,
            blake3_digest: None,
        };
        if let Err(error) = self.write_record(&record) {
            let _ = fs::remove_file(path);
            return Err(error);
        }
        Ok(source_ref)
    }

    pub fn append(&self, source_ref: &str, chunk: &[u8]) -> Result<(), MobileCoreError> {
        validate_source_ref(source_ref)?;
        if chunk.is_empty() || chunk.len() > MAX_CHUNK_BYTES {
            return Err(MobileCoreError::InvalidArgument(format!(
                "media stage chunk must contain 1..={MAX_CHUNK_BYTES} bytes"
            )));
        }
        let mut record = self.read_record(source_ref)?.ok_or_else(|| {
            MobileCoreError::InvalidArgument("private media stage does not exist".into())
        })?;
        if record.state != MediaStageState::Receiving {
            return Err(MobileCoreError::InvalidArgument(
                "private media stage is not receiving bytes".into(),
            ));
        }
        let chunk_bytes = u64::try_from(chunk.len()).unwrap_or(u64::MAX);
        if record.committed_plaintext_bytes.saturating_add(chunk_bytes) > MAX_SOURCE_BYTES {
            return Err(MobileCoreError::BudgetExceeded(format!(
                "private media source exceeds {MAX_SOURCE_BYTES} bytes"
            )));
        }

        let mut nonce_bytes = [0u8; NONCE_BYTES];
        getrandom::fill(&mut nonce_bytes)
            .map_err(|_| MobileCoreError::Security("media nonce CSPRNG unavailable".into()))?;
        let nonce = XNonce::from(nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: chunk,
                    aad: &chunk_aad(source_ref, record.committed_chunks),
                },
            )
            .map_err(|_| MobileCoreError::Security("cannot seal private media chunk".into()))?;
        let ciphertext_len = u32::try_from(ciphertext.len()).map_err(|_| {
            MobileCoreError::BudgetExceeded("encrypted media chunk length overflow".into())
        })?;

        let path = self.stage_path(source_ref);
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|error| storage_error("cannot open encrypted media stage", error))?;
        file.set_len(record.committed_file_bytes)
            .map_err(|error| storage_error("cannot fence media stage tail", error))?;
        file.seek(SeekFrom::Start(record.committed_file_bytes))
            .map_err(|error| storage_error("cannot seek media stage", error))?;
        file.write_all(&ciphertext_len.to_le_bytes())
            .and_then(|_| file.write_all(&nonce_bytes))
            .and_then(|_| file.write_all(&ciphertext))
            .map_err(|error| storage_error("cannot append encrypted media chunk", error))?;
        file.sync_data()
            .map_err(|error| storage_error("cannot sync encrypted media chunk", error))?;

        record.committed_chunks = record.committed_chunks.saturating_add(1);
        record.committed_plaintext_bytes =
            record.committed_plaintext_bytes.saturating_add(chunk_bytes);
        record.committed_file_bytes = record
            .committed_file_bytes
            .saturating_add(4 + NONCE_BYTES as u64 + u64::from(ciphertext_len));
        self.write_record(&record)
    }

    pub fn finish(&self, source_ref: &str) -> Result<MediaStageReceipt, MobileCoreError> {
        validate_source_ref(source_ref)?;
        let mut record = self.read_record(source_ref)?.ok_or_else(|| {
            MobileCoreError::InvalidArgument("private media stage does not exist".into())
        })?;
        if record.state == MediaStageState::StagedVerified {
            return receipt_from_record(&record);
        }
        if record.state != MediaStageState::Receiving
            || record.committed_plaintext_bytes == 0
            || record.committed_chunks == 0
        {
            return Err(MobileCoreError::InvalidArgument(
                "private media stage has no complete source".into(),
            ));
        }

        let verification = self.verify_stage(&record);
        let (detected_mime, digest) = match verification {
            Ok(value) => value,
            Err(error) => {
                record.state = MediaStageState::Rejected;
                record.detected_mime_type = None;
                record.blake3_digest = None;
                self.write_record(&record)?;
                let _ = fs::remove_file(self.stage_path(source_ref));
                return Err(error);
            }
        };
        let actual_class = media_class_for_mime(&detected_mime).ok_or_else(|| {
            MobileCoreError::InvalidArgument("unsupported private media type".into())
        })?;
        if actual_class != record.requested_class {
            record.state = MediaStageState::Rejected;
            self.write_record(&record)?;
            let _ = fs::remove_file(self.stage_path(source_ref));
            return Err(MobileCoreError::Security(
                "picked media bytes do not match the requested media class".into(),
            ));
        }
        if record.declared_mime_type != "application/octet-stream"
            && normalize_mime(&record.declared_mime_type) != normalize_mime(&detected_mime)
        {
            record.state = MediaStageState::Rejected;
            self.write_record(&record)?;
            let _ = fs::remove_file(self.stage_path(source_ref));
            return Err(MobileCoreError::Security(
                "picked media MIME claim does not match its bytes".into(),
            ));
        }

        record.state = MediaStageState::StagedVerified;
        record.detected_mime_type = Some(detected_mime);
        record.blake3_digest = Some(digest);
        self.write_record(&record)?;
        receipt_from_record(&record)
    }

    pub fn abort(&self, source_ref: &str) -> Result<(), MobileCoreError> {
        validate_source_ref(source_ref)?;
        let Some(mut record) = self.read_record(source_ref)? else {
            return Ok(());
        };
        if record.state == MediaStageState::StagedVerified {
            return Err(MobileCoreError::InvalidArgument(
                "verified private media stage cannot be aborted".into(),
            ));
        }
        record.state = MediaStageState::Interrupted;
        record.committed_chunks = 0;
        record.committed_plaintext_bytes = 0;
        record.committed_file_bytes = 0;
        self.write_record(&record)?;
        remove_file_if_present(&self.stage_path(source_ref))
    }

    pub fn inspect(&self, source_ref: &str) -> Result<Option<MediaStageReceipt>, MobileCoreError> {
        let Some(record) = self.read_record(source_ref)? else {
            return Ok(None);
        };
        if record.state != MediaStageState::StagedVerified {
            return Ok(None);
        }
        receipt_from_record(&record).map(Some)
    }

    pub fn staged_verified_count(&self) -> Result<u64, MobileCoreError> {
        self.count_state(MediaStageState::StagedVerified)
    }

    fn receiving_count(&self) -> Result<usize, MobileCoreError> {
        Ok(usize::try_from(self.count_state(MediaStageState::Receiving)?).unwrap_or(usize::MAX))
    }

    fn count_state(&self, state: MediaStageState) -> Result<u64, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(MEDIA_STAGES)?;
        let mut count = 0u64;
        for entry in table.iter()? {
            let (reference, sealed) = entry?;
            let record = self.open_record(reference.value(), sealed.value())?;
            if record.state == state {
                count = count.saturating_add(1);
            }
        }
        Ok(count)
    }

    fn recover_interrupted_stages(&self) -> Result<(), MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(MEDIA_STAGES)?;
        let mut interrupted = Vec::new();
        for entry in table.iter()? {
            let (reference, sealed) = entry?;
            let record = self.open_record(reference.value(), sealed.value())?;
            if record.state == MediaStageState::Receiving {
                interrupted.push(record);
            }
        }
        drop(table);
        drop(read);
        for mut record in interrupted {
            remove_file_if_present(&self.stage_path(&record.source_ref))?;
            record.state = MediaStageState::Interrupted;
            record.committed_chunks = 0;
            record.committed_plaintext_bytes = 0;
            record.committed_file_bytes = 0;
            self.write_record(&record)?;
        }
        Ok(())
    }

    fn verify_stage(&self, record: &MediaStageRecord) -> Result<(String, String), MobileCoreError> {
        let path = self.stage_path(&record.source_ref);
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|error| storage_error("cannot verify encrypted media stage", error))?;
        file.set_len(record.committed_file_bytes)
            .map_err(|error| storage_error("cannot truncate uncommitted media tail", error))?;
        let mut magic = [0u8; FILE_MAGIC.len()];
        file.read_exact(&mut magic)
            .map_err(|error| storage_error("cannot read media stage header", error))?;
        if &magic != FILE_MAGIC {
            return Err(MobileCoreError::Security(
                "encrypted media stage header is invalid".into(),
            ));
        }

        let mut hasher = blake3::Hasher::new();
        let mut prefix = Vec::with_capacity(SNIFF_PREFIX_BYTES);
        let mut plaintext_bytes = 0u64;
        for chunk_index in 0..record.committed_chunks {
            let mut length_bytes = [0u8; 4];
            file.read_exact(&mut length_bytes)
                .map_err(|error| storage_error("cannot read media chunk length", error))?;
            let ciphertext_len = u32::from_le_bytes(length_bytes) as usize;
            if ciphertext_len < 16 || ciphertext_len > MAX_CHUNK_BYTES + 16 {
                return Err(MobileCoreError::Security(
                    "encrypted media chunk length is invalid".into(),
                ));
            }
            let mut nonce_bytes = [0u8; NONCE_BYTES];
            file.read_exact(&mut nonce_bytes)
                .map_err(|error| storage_error("cannot read media chunk nonce", error))?;
            let mut ciphertext = vec![0u8; ciphertext_len];
            file.read_exact(&mut ciphertext)
                .map_err(|error| storage_error("cannot read encrypted media chunk", error))?;
            let plaintext = Zeroizing::new(
                self.cipher
                    .decrypt(
                        &XNonce::from(nonce_bytes),
                        Payload {
                            msg: &ciphertext,
                            aad: &chunk_aad(&record.source_ref, chunk_index),
                        },
                    )
                    .map_err(|_| {
                        MobileCoreError::Security(
                            "encrypted private media chunk authentication failed".into(),
                        )
                    })?,
            );
            plaintext_bytes =
                plaintext_bytes.saturating_add(u64::try_from(plaintext.len()).unwrap_or(u64::MAX));
            hasher.update(&plaintext);
            if prefix.len() < SNIFF_PREFIX_BYTES {
                let remaining = SNIFF_PREFIX_BYTES - prefix.len();
                prefix.extend_from_slice(&plaintext[..plaintext.len().min(remaining)]);
            }
        }
        if plaintext_bytes != record.committed_plaintext_bytes
            || file
                .stream_position()
                .map_err(|error| storage_error("cannot inspect media stage position", error))?
                != record.committed_file_bytes
        {
            return Err(MobileCoreError::Security(
                "encrypted media stage length commitment is invalid".into(),
            ));
        }
        let detected = infer::get(&prefix)
            .map(|kind| kind.mime_type().to_ascii_lowercase())
            .ok_or_else(|| {
                MobileCoreError::InvalidArgument(
                    "private media type is unknown or unsupported".into(),
                )
            })?;
        Ok((
            normalize_mime(&detected).to_owned(),
            hasher.finalize().to_hex().to_string(),
        ))
    }

    fn read_record(&self, source_ref: &str) -> Result<Option<MediaStageRecord>, MobileCoreError> {
        validate_source_ref(source_ref)?;
        let read = self.database.begin_read()?;
        let table = read.open_table(MEDIA_STAGES)?;
        let Some(sealed) = table.get(source_ref)? else {
            return Ok(None);
        };
        self.open_record(source_ref, sealed.value()).map(Some)
    }

    fn write_record(&self, record: &MediaStageRecord) -> Result<(), MobileCoreError> {
        let plaintext = Zeroizing::new(serde_json::to_vec(record)?);
        let sealed = self.seal_metadata(&record.source_ref, &plaintext)?;
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(MEDIA_STAGES)?;
            table.insert(record.source_ref.as_str(), sealed.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }

    fn open_record(
        &self,
        source_ref: &str,
        sealed: &[u8],
    ) -> Result<MediaStageRecord, MobileCoreError> {
        if sealed.len() < 1 + NONCE_BYTES + 16 || sealed[0] != FORMAT_VERSION {
            return Err(MobileCoreError::Security(
                "private media metadata envelope is invalid".into(),
            ));
        }
        let nonce = XNonce::try_from(&sealed[1..1 + NONCE_BYTES])
            .map_err(|_| MobileCoreError::Security("media metadata nonce is invalid".into()))?;
        let plaintext = Zeroizing::new(
            self.cipher
                .decrypt(
                    &nonce,
                    Payload {
                        msg: &sealed[1 + NONCE_BYTES..],
                        aad: &metadata_aad(source_ref),
                    },
                )
                .map_err(|_| {
                    MobileCoreError::Security("private media metadata authentication failed".into())
                })?,
        );
        let record: MediaStageRecord = serde_json::from_slice(&plaintext)?;
        if record.format_version != FORMAT_VERSION || record.source_ref != source_ref {
            return Err(MobileCoreError::Security(
                "private media metadata binding is invalid".into(),
            ));
        }
        Ok(record)
    }

    fn seal_metadata(
        &self,
        source_ref: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, MobileCoreError> {
        let mut nonce_bytes = [0u8; NONCE_BYTES];
        getrandom::fill(&mut nonce_bytes)
            .map_err(|_| MobileCoreError::Security("media nonce CSPRNG unavailable".into()))?;
        let ciphertext = self
            .cipher
            .encrypt(
                &XNonce::from(nonce_bytes),
                Payload {
                    msg: plaintext,
                    aad: &metadata_aad(source_ref),
                },
            )
            .map_err(|_| MobileCoreError::Security("cannot seal media metadata".into()))?;
        let mut sealed = Vec::with_capacity(1 + NONCE_BYTES + ciphertext.len());
        sealed.push(FORMAT_VERSION);
        sealed.extend_from_slice(&nonce_bytes);
        sealed.extend_from_slice(&ciphertext);
        Ok(sealed)
    }

    fn stage_path(&self, source_ref: &str) -> PathBuf {
        self.root.join(format!("{source_ref}.obmstg"))
    }
}

fn receipt_from_record(record: &MediaStageRecord) -> Result<MediaStageReceipt, MobileCoreError> {
    Ok(MediaStageReceipt {
        source_ref: record.source_ref.clone(),
        media_class: record.requested_class.clone(),
        mime_type: record.detected_mime_type.clone().ok_or_else(|| {
            MobileCoreError::Security("verified media stage lost its MIME binding".into())
        })?,
        content_bytes: record.committed_plaintext_bytes,
        blake3_digest: record.blake3_digest.clone().ok_or_else(|| {
            MobileCoreError::Security("verified media stage lost its digest binding".into())
        })?,
        state: record.state,
    })
}

fn validate_requested_class(value: &str) -> Result<(), MobileCoreError> {
    if matches!(value, "image" | "video" | "audio" | "document") {
        Ok(())
    } else {
        Err(MobileCoreError::InvalidArgument(
            "requested media class is unsupported".into(),
        ))
    }
}

fn validate_declared_mime(value: &str) -> Result<(), MobileCoreError> {
    if value.is_empty()
        || value.len() > MAX_DECLARED_MIME_BYTES
        || !value.is_ascii()
        || value.contains('|')
        || value.contains('\0')
    {
        return Err(MobileCoreError::InvalidArgument(
            "declared media MIME type is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_source_ref(value: &str) -> Result<(), MobileCoreError> {
    let Some(hex_value) = value.strip_prefix("source_") else {
        return Err(MobileCoreError::InvalidArgument(
            "private media source reference is invalid".into(),
        ));
    };
    if hex_value.len() != SOURCE_ID_BYTES * 2
        || !hex_value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(MobileCoreError::InvalidArgument(
            "private media source reference is invalid".into(),
        ));
    }
    Ok(())
}

fn random_source_ref() -> Result<String, MobileCoreError> {
    let mut bytes = [0u8; SOURCE_ID_BYTES];
    getrandom::fill(&mut bytes)
        .map_err(|_| MobileCoreError::Security("media source CSPRNG unavailable".into()))?;
    Ok(format!("source_{}", hex::encode(bytes)))
}

fn media_class_for_mime(mime: &str) -> Option<&'static str> {
    if mime.starts_with("image/") {
        Some("image")
    } else if mime.starts_with("video/") {
        Some("video")
    } else if mime.starts_with("audio/") {
        Some("audio")
    } else if matches!(
        mime,
        "application/pdf"
            | "application/msword"
            | "application/vnd.ms-excel"
            | "application/vnd.ms-powerpoint"
            | "application/rtf"
    ) {
        Some("document")
    } else {
        None
    }
}

fn normalize_mime(mime: &str) -> &str {
    match mime {
        "image/jpg" => "image/jpeg",
        "audio/x-wav" => "audio/wav",
        other => other,
    }
}

fn metadata_aad(source_ref: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(METADATA_AAD_CONTEXT.len() + source_ref.len());
    aad.extend_from_slice(METADATA_AAD_CONTEXT);
    aad.extend_from_slice(source_ref.as_bytes());
    aad
}

fn chunk_aad(source_ref: &str, chunk_index: u64) -> Vec<u8> {
    let mut aad =
        Vec::with_capacity(CHUNK_AAD_CONTEXT.len() + source_ref.len() + std::mem::size_of::<u64>());
    aad.extend_from_slice(CHUNK_AAD_CONTEXT);
    aad.extend_from_slice(source_ref.as_bytes());
    aad.extend_from_slice(&chunk_index.to_le_bytes());
    aad
}

fn create_dir_all(path: &Path) -> Result<(), MobileCoreError> {
    fs::create_dir_all(path)
        .map_err(|error| storage_error("cannot create private media directory", error))
}

fn remove_file_if_present(path: &Path) -> Result<(), MobileCoreError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(storage_error("cannot remove private media stage", error)),
    }
}

fn storage_error(context: &str, error: std::io::Error) -> MobileCoreError {
    MobileCoreError::Storage(format!("{context}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG_BYTES: &[u8] = &[
        0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, b'I', b'H', b'D',
        b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89,
    ];

    fn open_store(root: &Path) -> MediaStagingStore {
        MediaStagingStore::open(
            &root.join("staging.redb"),
            &root.join("media").join("staging"),
            MediaStagingKey::derive(&[7u8; 32]),
        )
        .expect("open staging store")
    }

    #[test]
    fn stages_encrypted_media_and_returns_only_verified_opaque_receipt() {
        let root = tempfile::tempdir().expect("tempdir");
        let store = open_store(root.path());
        let source_ref = store.start("image", "image/png", 12).expect("start stage");
        store.append(&source_ref, PNG_BYTES).expect("append");

        let encrypted = fs::read(store.stage_path(&source_ref)).expect("read stage");
        assert!(!encrypted
            .windows(PNG_BYTES.len())
            .any(|value| value == PNG_BYTES));

        let receipt = store.finish(&source_ref).expect("finish stage");
        assert_eq!(receipt.source_ref, source_ref);
        assert_eq!(receipt.media_class, "image");
        assert_eq!(receipt.mime_type, "image/png");
        assert_eq!(receipt.content_bytes, PNG_BYTES.len() as u64);
        assert_eq!(
            receipt.blake3_digest,
            blake3::hash(PNG_BYTES).to_hex().to_string()
        );
        assert_eq!(store.staged_verified_count().expect("count"), 1);
    }

    #[test]
    fn interrupted_receiving_stage_is_removed_during_recovery() {
        let root = tempfile::tempdir().expect("tempdir");
        let source_ref = {
            let store = open_store(root.path());
            let source_ref = store.start("image", "image/png", 12).expect("start stage");
            store.append(&source_ref, PNG_BYTES).expect("append");
            source_ref
        };

        let recovered = open_store(root.path());
        assert_eq!(recovered.staged_verified_count().expect("count"), 0);
        assert!(!recovered.stage_path(&source_ref).exists());
        let record = recovered
            .read_record(&source_ref)
            .expect("read")
            .expect("record");
        assert_eq!(record.state, MediaStageState::Interrupted);
        assert_eq!(record.committed_plaintext_bytes, 0);
    }

    #[test]
    fn rejects_claimed_mime_or_requested_class_mismatch() {
        let root = tempfile::tempdir().expect("tempdir");
        let store = open_store(root.path());
        let source_ref = store.start("video", "video/mp4", 12).expect("start stage");
        store.append(&source_ref, PNG_BYTES).expect("append");
        assert!(matches!(
            store.finish(&source_ref),
            Err(MobileCoreError::Security(_))
        ));
        assert!(!store.stage_path(&source_ref).exists());
    }

    #[test]
    fn verifies_supported_image_video_audio_and_pdf_classes_from_bytes() {
        let root = tempfile::tempdir().expect("tempdir");
        let store = open_store(root.path());
        for (media_class, declared_mime, bytes, expected_mime) in [
            ("image", "image/png", PNG_BYTES, "image/png"),
            (
                "video",
                "video/mp4",
                b"\x00\x00\x00\x18ftypisom\x00\x00\x00\x00isomiso2" as &[u8],
                "video/mp4",
            ),
            (
                "audio",
                "audio/wav",
                b"RIFF\x24\x00\x00\x00WAVEfmt \x10\x00\x00\x00" as &[u8],
                "audio/wav",
            ),
            (
                "document",
                "application/pdf",
                b"%PDF-1.7\n1 0 obj\n<<>>\nendobj\n" as &[u8],
                "application/pdf",
            ),
        ] {
            let source_ref = store
                .start(media_class, declared_mime, 12)
                .expect("start stage");
            store.append(&source_ref, bytes).expect("append");
            let receipt = store.finish(&source_ref).expect("finish");
            assert_eq!(receipt.media_class, media_class);
            assert_eq!(receipt.mime_type, expected_mime);
        }
    }

    #[test]
    fn bounds_chunks_and_rejects_archive_bytes() {
        let root = tempfile::tempdir().expect("tempdir");
        let store = open_store(root.path());
        let source_ref = store
            .start("document", "application/zip", 12)
            .expect("start stage");
        assert!(store
            .append(&source_ref, &vec![1u8; MAX_CHUNK_BYTES + 1])
            .is_err());
        store
            .append(&source_ref, b"PK\x03\x04unsafe archive")
            .expect("append");
        assert!(store.finish(&source_ref).is_err());
    }
}
