use std::{
    fmt,
    fs::{self, File},
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::MobileCoreError;

const ARCHIVE_MAGIC: &[u8; 8] = b"OBARV001";
const ARCHIVE_CHUNK_BYTES: usize = 1024 * 1024;
const ARCHIVE_KEY_CONTEXT: &str = "onebrain:mobile:portable-archive-key:1";
const ARCHIVE_AAD_CONTEXT: &[u8] = b"onebrain:mobile:portable-archive:1\0";
const MAX_ARCHIVE_CHUNKS: usize = 1_000_000;
const MAX_MANIFEST_CIPHERTEXT_BYTES: usize = 64 * 1024;
const ARCHIVE_TAG_BYTES: usize = 16;

pub const MOBILE_ARCHIVE_VERSION: u16 = 1;

pub struct RecoveryKey(Zeroizing<[u8; 32]>);

impl RecoveryKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self, MobileCoreError> {
        if bytes.iter().all(|byte| *byte == 0) {
            return Err(MobileCoreError::Archive(
                "recovery key cannot be all zero".into(),
            ));
        }
        Ok(Self(Zeroizing::new(bytes)))
    }

    fn derived_archive_key(&self) -> [u8; 32] {
        blake3::derive_key(ARCHIVE_KEY_CONTEXT, self.0.as_slice())
    }
}

impl fmt::Debug for RecoveryKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RecoveryKey([REDACTED])")
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EncryptedArchivePayload {
    pub archive_kind: String,
    pub source_schema_version: u32,
    pub canonical_bytes: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct EncryptedArchiveManifest {
    archive_version: u16,
    archive_kind: String,
    source_schema_version: u32,
    payload_length: u64,
    payload_digest: String,
    chunk_bytes: u32,
    chunk_count: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncryptedArchiveInspection {
    pub archive_version: u16,
    pub archive_kind: String,
    pub source_schema_version: u32,
    pub payload_length: u64,
    pub payload_digest: String,
    pub chunk_count: u32,
}

pub fn create_encrypted_archive(
    payload: &EncryptedArchivePayload,
    recovery_key: &RecoveryKey,
) -> Result<Vec<u8>, MobileCoreError> {
    validate_archive_kind(&payload.archive_kind)?;
    let payload_bytes = &payload.canonical_bytes;
    let chunk_count = payload_bytes.len().div_ceil(ARCHIVE_CHUNK_BYTES);
    if chunk_count > MAX_ARCHIVE_CHUNKS {
        return Err(MobileCoreError::Archive(
            "archive exceeds the bounded chunk count".into(),
        ));
    }
    let manifest = EncryptedArchiveManifest {
        archive_version: MOBILE_ARCHIVE_VERSION,
        archive_kind: payload.archive_kind.clone(),
        source_schema_version: payload.source_schema_version,
        payload_length: u64::try_from(payload_bytes.len())
            .map_err(|_| MobileCoreError::Archive("payload length overflow".into()))?,
        payload_digest: blake3::hash(payload_bytes).to_hex().to_string(),
        chunk_bytes: ARCHIVE_CHUNK_BYTES as u32,
        chunk_count: u32::try_from(chunk_count)
            .map_err(|_| MobileCoreError::Archive("chunk count overflow".into()))?,
    };
    let manifest_bytes = serde_json::to_vec(&manifest)?;
    let mut nonce_prefix = [0u8; 16];
    getrandom::fill(&mut nonce_prefix)
        .map_err(|error| MobileCoreError::Archive(format!("OS random failed: {error}")))?;
    let key = Zeroizing::new(recovery_key.derived_archive_key());
    let cipher = XChaCha20Poly1305::new((&*key).into());
    let manifest_ciphertext = seal_part(&cipher, &nonce_prefix, 0, &manifest_bytes)?;

    let mut output = Vec::with_capacity(
        8 + 2 + 16 + 4 + manifest_ciphertext.len() + 4 + payload_bytes.len() + chunk_count * 20,
    );
    output.extend_from_slice(ARCHIVE_MAGIC);
    output.extend_from_slice(&MOBILE_ARCHIVE_VERSION.to_be_bytes());
    output.extend_from_slice(&nonce_prefix);
    push_u32(&mut output, manifest_ciphertext.len())?;
    output.extend_from_slice(&manifest_ciphertext);
    push_u32(&mut output, chunk_count)?;
    for (index, chunk) in payload_bytes.chunks(ARCHIVE_CHUNK_BYTES).enumerate() {
        let ciphertext = seal_part(
            &cipher,
            &nonce_prefix,
            u64::try_from(index).unwrap_or(u64::MAX).saturating_add(1),
            chunk,
        )?;
        push_u32(&mut output, ciphertext.len())?;
        output.extend_from_slice(&ciphertext);
    }
    Ok(output)
}

pub fn inspect_encrypted_archive(
    archive: &[u8],
    recovery_key: &RecoveryKey,
) -> Result<EncryptedArchiveInspection, MobileCoreError> {
    let parsed = ParsedArchive::parse(archive, recovery_key)?;
    Ok(parsed.inspection())
}

pub fn open_encrypted_archive(
    archive: &[u8],
    recovery_key: &RecoveryKey,
) -> Result<EncryptedArchivePayload, MobileCoreError> {
    let parsed = ParsedArchive::parse(archive, recovery_key)?;
    let expected_length = usize::try_from(parsed.manifest.payload_length)
        .map_err(|_| MobileCoreError::Archive("payload is too large for this platform".into()))?;
    let mut plaintext = Vec::with_capacity(expected_length);
    for (index, ciphertext) in parsed.chunks.iter().enumerate() {
        let part = parsed
            .cipher
            .decrypt(
                &nonce(&parsed.nonce_prefix, index as u64 + 1),
                Payload {
                    msg: ciphertext,
                    aad: &part_aad(index as u64 + 1),
                },
            )
            .map_err(|_| MobileCoreError::Archive("wrong recovery key or corrupt chunk".into()))?;
        plaintext.extend_from_slice(&part);
    }
    if plaintext.len() != expected_length {
        return Err(MobileCoreError::Archive(
            "archive payload length mismatch".into(),
        ));
    }
    let digest = blake3::hash(&plaintext).to_hex().to_string();
    if digest != parsed.manifest.payload_digest {
        return Err(MobileCoreError::Archive(
            "archive payload digest mismatch".into(),
        ));
    }
    Ok(EncryptedArchivePayload {
        archive_kind: parsed.manifest.archive_kind,
        source_schema_version: parsed.manifest.source_schema_version,
        canonical_bytes: plaintext,
    })
}

pub fn create_encrypted_archive_file(
    source: &Path,
    destination: &Path,
    archive_kind: &str,
    source_schema_version: u32,
    recovery_key: &RecoveryKey,
) -> Result<EncryptedArchiveInspection, MobileCoreError> {
    validate_archive_kind(archive_kind)?;
    if source == destination {
        return Err(MobileCoreError::Archive(
            "archive source and destination must differ".into(),
        ));
    }
    if destination.exists() {
        return Err(MobileCoreError::Archive(
            "refusing to overwrite an existing archive".into(),
        ));
    }
    let (payload_length, payload_digest) = digest_file(source)?;
    let payload_length_usize = usize::try_from(payload_length)
        .map_err(|_| MobileCoreError::Archive("source is too large for this platform".into()))?;
    let chunk_count = payload_length_usize.div_ceil(ARCHIVE_CHUNK_BYTES);
    if chunk_count > MAX_ARCHIVE_CHUNKS {
        return Err(MobileCoreError::Archive(
            "archive exceeds the bounded chunk count".into(),
        ));
    }
    let manifest = EncryptedArchiveManifest {
        archive_version: MOBILE_ARCHIVE_VERSION,
        archive_kind: archive_kind.to_owned(),
        source_schema_version,
        payload_length,
        payload_digest,
        chunk_bytes: ARCHIVE_CHUNK_BYTES as u32,
        chunk_count: u32::try_from(chunk_count)
            .map_err(|_| MobileCoreError::Archive("chunk count overflow".into()))?,
    };
    let temporary = creating_path(destination);
    if temporary.exists() {
        return Err(MobileCoreError::Archive(
            "an interrupted archive creation requires explicit recovery".into(),
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            MobileCoreError::Archive(format!(
                "cannot create archive directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    let result = (|| {
        let manifest_bytes = serde_json::to_vec(&manifest)?;
        let mut nonce_prefix = [0u8; 16];
        getrandom::fill(&mut nonce_prefix)
            .map_err(|error| MobileCoreError::Archive(format!("OS random failed: {error}")))?;
        let key = Zeroizing::new(recovery_key.derived_archive_key());
        let cipher = XChaCha20Poly1305::new((&*key).into());
        let encrypted_manifest = seal_part(&cipher, &nonce_prefix, 0, &manifest_bytes)?;
        let mut output = BufWriter::new(File::create(&temporary).map_err(archive_io)?);
        output.write_all(ARCHIVE_MAGIC).map_err(archive_io)?;
        output
            .write_all(&MOBILE_ARCHIVE_VERSION.to_be_bytes())
            .map_err(archive_io)?;
        output.write_all(&nonce_prefix).map_err(archive_io)?;
        write_u32(&mut output, encrypted_manifest.len())?;
        output.write_all(&encrypted_manifest).map_err(archive_io)?;
        write_u32(&mut output, chunk_count)?;

        let mut source = BufReader::new(File::open(source).map_err(archive_io)?);
        let mut buffer = vec![0u8; ARCHIVE_CHUNK_BYTES];
        for index in 0..chunk_count {
            let read = read_chunk(&mut source, &mut buffer)?;
            if read == 0 {
                return Err(MobileCoreError::Archive(
                    "source changed while the archive was created".into(),
                ));
            }
            let encrypted = seal_part(&cipher, &nonce_prefix, index as u64 + 1, &buffer[..read])?;
            write_u32(&mut output, encrypted.len())?;
            output.write_all(&encrypted).map_err(archive_io)?;
        }
        if read_chunk(&mut source, &mut buffer)? != 0 {
            return Err(MobileCoreError::Archive(
                "source grew while the archive was created".into(),
            ));
        }
        output.flush().map_err(archive_io)?;
        output.get_ref().sync_all().map_err(archive_io)?;
        drop(output);
        fs::rename(&temporary, destination).map_err(archive_io)?;
        sync_parent_directory(destination)?;
        Ok(inspection_from_manifest(&manifest))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn inspect_encrypted_archive_file(
    archive: &Path,
    recovery_key: &RecoveryKey,
) -> Result<EncryptedArchiveInspection, MobileCoreError> {
    let mut input = BufReader::new(File::open(archive).map_err(archive_io)?);
    let (manifest, _, _, _) = read_file_manifest(&mut input, recovery_key)?;
    Ok(inspection_from_manifest(&manifest))
}

pub fn open_encrypted_archive_file(
    archive: &Path,
    destination: &Path,
    recovery_key: &RecoveryKey,
) -> Result<EncryptedArchiveInspection, MobileCoreError> {
    if archive == destination {
        return Err(MobileCoreError::Archive(
            "archive and restore destination must differ".into(),
        ));
    }
    if destination.exists() {
        return Err(MobileCoreError::Archive(
            "refusing to overwrite an existing restore destination".into(),
        ));
    }
    let temporary = creating_path(destination);
    if temporary.exists() {
        return Err(MobileCoreError::Archive(
            "an interrupted restore requires explicit recovery".into(),
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(archive_io)?;
    }
    let result = (|| {
        let mut input = BufReader::new(File::open(archive).map_err(archive_io)?);
        let (manifest, nonce_prefix, cipher, chunk_count) =
            read_file_manifest(&mut input, recovery_key)?;
        let mut output = BufWriter::new(File::create(&temporary).map_err(archive_io)?);
        let mut hasher = blake3::Hasher::new();
        let mut restored_length = 0u64;
        for index in 0..chunk_count {
            let ciphertext_length = read_u32(&mut input)? as usize;
            if !(ARCHIVE_TAG_BYTES..=ARCHIVE_CHUNK_BYTES + ARCHIVE_TAG_BYTES)
                .contains(&ciphertext_length)
            {
                return Err(MobileCoreError::Archive(
                    "archive chunk length is outside its profile".into(),
                ));
            }
            let mut ciphertext = vec![0u8; ciphertext_length];
            input.read_exact(&mut ciphertext).map_err(archive_io)?;
            let plaintext = cipher
                .decrypt(
                    &nonce(&nonce_prefix, index as u64 + 1),
                    Payload {
                        msg: &ciphertext,
                        aad: &part_aad(index as u64 + 1),
                    },
                )
                .map_err(|_| {
                    MobileCoreError::Archive("wrong recovery key or corrupt chunk".into())
                })?;
            restored_length = restored_length
                .checked_add(plaintext.len() as u64)
                .ok_or_else(|| MobileCoreError::Archive("restored length overflow".into()))?;
            hasher.update(&plaintext);
            output.write_all(&plaintext).map_err(archive_io)?;
        }
        let mut trailing = [0u8; 1];
        if input.read(&mut trailing).map_err(archive_io)? != 0 {
            return Err(MobileCoreError::Archive(
                "archive has unbound trailing bytes".into(),
            ));
        }
        if restored_length != manifest.payload_length
            || hasher.finalize().to_hex().as_str() != manifest.payload_digest
        {
            return Err(MobileCoreError::Archive(
                "restored payload length or digest mismatch".into(),
            ));
        }
        output.flush().map_err(archive_io)?;
        output.get_ref().sync_all().map_err(archive_io)?;
        drop(output);
        fs::rename(&temporary, destination).map_err(archive_io)?;
        sync_parent_directory(destination)?;
        Ok(inspection_from_manifest(&manifest))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn digest_file(source: &Path) -> Result<(u64, String), MobileCoreError> {
    let mut input = BufReader::new(File::open(source).map_err(archive_io)?);
    let mut buffer = vec![0u8; ARCHIVE_CHUNK_BYTES];
    let mut length = 0u64;
    let mut hasher = blake3::Hasher::new();
    loop {
        let read = read_chunk(&mut input, &mut buffer)?;
        if read == 0 {
            break;
        }
        length = length
            .checked_add(read as u64)
            .ok_or_else(|| MobileCoreError::Archive("source length overflow".into()))?;
        hasher.update(&buffer[..read]);
    }
    Ok((length, hasher.finalize().to_hex().to_string()))
}

fn read_file_manifest(
    input: &mut impl Read,
    recovery_key: &RecoveryKey,
) -> Result<(EncryptedArchiveManifest, [u8; 16], XChaCha20Poly1305, usize), MobileCoreError> {
    let mut magic = [0u8; 8];
    input.read_exact(&mut magic).map_err(archive_io)?;
    if &magic != ARCHIVE_MAGIC {
        return Err(MobileCoreError::Archive("archive magic mismatch".into()));
    }
    let version = read_u16(input)?;
    if version != MOBILE_ARCHIVE_VERSION {
        return Err(MobileCoreError::Archive(format!(
            "unsupported archive version {version}"
        )));
    }
    let mut nonce_prefix = [0u8; 16];
    input.read_exact(&mut nonce_prefix).map_err(archive_io)?;
    let manifest_length = read_u32(input)? as usize;
    if !(ARCHIVE_TAG_BYTES..=MAX_MANIFEST_CIPHERTEXT_BYTES).contains(&manifest_length) {
        return Err(MobileCoreError::Archive(
            "archive manifest length is outside its profile".into(),
        ));
    }
    let mut encrypted_manifest = vec![0u8; manifest_length];
    input
        .read_exact(&mut encrypted_manifest)
        .map_err(archive_io)?;
    let key = Zeroizing::new(recovery_key.derived_archive_key());
    let cipher = XChaCha20Poly1305::new((&*key).into());
    let manifest_bytes = cipher
        .decrypt(
            &nonce(&nonce_prefix, 0),
            Payload {
                msg: &encrypted_manifest,
                aad: &part_aad(0),
            },
        )
        .map_err(|_| MobileCoreError::Archive("wrong recovery key or corrupt manifest".into()))?;
    let manifest: EncryptedArchiveManifest = serde_json::from_slice(&manifest_bytes)?;
    validate_manifest(&manifest, version)?;
    let encoded_chunk_count = read_u32(input)? as usize;
    if encoded_chunk_count != manifest.chunk_count as usize {
        return Err(MobileCoreError::Archive(
            "archive chunk count mismatch".into(),
        ));
    }
    Ok((manifest, nonce_prefix, cipher, encoded_chunk_count))
}

fn validate_manifest(
    manifest: &EncryptedArchiveManifest,
    version: u16,
) -> Result<(), MobileCoreError> {
    if manifest.archive_version != version
        || manifest.chunk_bytes != ARCHIVE_CHUNK_BYTES as u32
        || manifest.chunk_count as usize > MAX_ARCHIVE_CHUNKS
    {
        return Err(MobileCoreError::Archive(
            "archive manifest profile mismatch".into(),
        ));
    }
    validate_archive_kind(&manifest.archive_kind)
}

fn inspection_from_manifest(manifest: &EncryptedArchiveManifest) -> EncryptedArchiveInspection {
    EncryptedArchiveInspection {
        archive_version: manifest.archive_version,
        archive_kind: manifest.archive_kind.clone(),
        source_schema_version: manifest.source_schema_version,
        payload_length: manifest.payload_length,
        payload_digest: manifest.payload_digest.clone(),
        chunk_count: manifest.chunk_count,
    }
}

fn creating_path(destination: &Path) -> PathBuf {
    let mut value = destination.as_os_str().to_os_string();
    value.push(".creating");
    PathBuf::from(value)
}

fn archive_io(error: std::io::Error) -> MobileCoreError {
    MobileCoreError::Archive(error.to_string())
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> Result<(), MobileCoreError> {
    if let Some(parent) = path.parent() {
        File::open(parent)
            .map_err(archive_io)?
            .sync_all()
            .map_err(archive_io)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> Result<(), MobileCoreError> {
    Ok(())
}

fn read_chunk(reader: &mut impl Read, buffer: &mut [u8]) -> Result<usize, MobileCoreError> {
    let mut filled = 0;
    while filled < buffer.len() {
        let read = reader.read(&mut buffer[filled..]).map_err(archive_io)?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    Ok(filled)
}

fn read_u16(reader: &mut impl Read) -> Result<u16, MobileCoreError> {
    let mut value = [0u8; 2];
    reader.read_exact(&mut value).map_err(archive_io)?;
    Ok(u16::from_be_bytes(value))
}

fn read_u32(reader: &mut impl Read) -> Result<u32, MobileCoreError> {
    let mut value = [0u8; 4];
    reader.read_exact(&mut value).map_err(archive_io)?;
    Ok(u32::from_be_bytes(value))
}

fn write_u32(writer: &mut impl Write, value: usize) -> Result<(), MobileCoreError> {
    let value = u32::try_from(value)
        .map_err(|_| MobileCoreError::Archive("archive field exceeds u32".into()))?;
    writer.write_all(&value.to_be_bytes()).map_err(archive_io)
}

struct ParsedArchive<'a> {
    manifest: EncryptedArchiveManifest,
    nonce_prefix: [u8; 16],
    chunks: Vec<&'a [u8]>,
    cipher: XChaCha20Poly1305,
}

impl<'a> ParsedArchive<'a> {
    fn parse(archive: &'a [u8], recovery_key: &RecoveryKey) -> Result<Self, MobileCoreError> {
        let mut cursor = ArchiveCursor::new(archive);
        if cursor.take(ARCHIVE_MAGIC.len())? != ARCHIVE_MAGIC {
            return Err(MobileCoreError::Archive("archive magic mismatch".into()));
        }
        let version = cursor.u16()?;
        if version != MOBILE_ARCHIVE_VERSION {
            return Err(MobileCoreError::Archive(format!(
                "unsupported archive version {version}"
            )));
        }
        let mut nonce_prefix = [0u8; 16];
        nonce_prefix.copy_from_slice(cursor.take(16)?);
        let manifest_ciphertext = cursor.length_prefixed()?;
        let key = Zeroizing::new(recovery_key.derived_archive_key());
        let cipher = XChaCha20Poly1305::new((&*key).into());
        let manifest_bytes = cipher
            .decrypt(
                &nonce(&nonce_prefix, 0),
                Payload {
                    msg: manifest_ciphertext,
                    aad: &part_aad(0),
                },
            )
            .map_err(|_| {
                MobileCoreError::Archive("wrong recovery key or corrupt manifest".into())
            })?;
        let manifest: EncryptedArchiveManifest = serde_json::from_slice(&manifest_bytes)?;
        validate_manifest(&manifest, version)?;
        let encoded_chunk_count = cursor.u32()? as usize;
        if encoded_chunk_count != manifest.chunk_count as usize {
            return Err(MobileCoreError::Archive(
                "archive chunk count mismatch".into(),
            ));
        }
        let mut chunks = Vec::with_capacity(encoded_chunk_count);
        for _ in 0..encoded_chunk_count {
            chunks.push(cursor.length_prefixed()?);
        }
        if !cursor.is_finished() {
            return Err(MobileCoreError::Archive(
                "archive has unbound trailing bytes".into(),
            ));
        }
        Ok(Self {
            manifest,
            nonce_prefix,
            chunks,
            cipher,
        })
    }

    fn inspection(&self) -> EncryptedArchiveInspection {
        inspection_from_manifest(&self.manifest)
    }
}

struct ArchiveCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> ArchiveCursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], MobileCoreError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| MobileCoreError::Archive("archive length overflow".into()))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| MobileCoreError::Archive("truncated archive".into()))?;
        self.offset = end;
        Ok(value)
    }

    fn u16(&mut self) -> Result<u16, MobileCoreError> {
        let mut value = [0u8; 2];
        value.copy_from_slice(self.take(2)?);
        Ok(u16::from_be_bytes(value))
    }

    fn u32(&mut self) -> Result<u32, MobileCoreError> {
        let mut value = [0u8; 4];
        value.copy_from_slice(self.take(4)?);
        Ok(u32::from_be_bytes(value))
    }

    fn length_prefixed(&mut self) -> Result<&'a [u8], MobileCoreError> {
        let length = self.u32()? as usize;
        self.take(length)
    }

    fn is_finished(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

fn validate_archive_kind(value: &str) -> Result<(), MobileCoreError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(MobileCoreError::Archive(
            "archive_kind must be bounded lowercase ASCII".into(),
        ));
    }
    Ok(())
}

fn seal_part(
    cipher: &XChaCha20Poly1305,
    nonce_prefix: &[u8; 16],
    index: u64,
    plaintext: &[u8],
) -> Result<Vec<u8>, MobileCoreError> {
    cipher
        .encrypt(
            &nonce(nonce_prefix, index),
            Payload {
                msg: plaintext,
                aad: &part_aad(index),
            },
        )
        .map_err(|_| MobileCoreError::Archive("archive encryption failed".into()))
}

fn nonce(prefix: &[u8; 16], index: u64) -> XNonce {
    let mut bytes = [0u8; 24];
    bytes[..16].copy_from_slice(prefix);
    bytes[16..].copy_from_slice(&index.to_be_bytes());
    bytes.into()
}

fn part_aad(index: u64) -> Vec<u8> {
    let mut aad = Vec::with_capacity(ARCHIVE_AAD_CONTEXT.len() + 10);
    aad.extend_from_slice(ARCHIVE_AAD_CONTEXT);
    aad.extend_from_slice(&MOBILE_ARCHIVE_VERSION.to_be_bytes());
    aad.extend_from_slice(&index.to_be_bytes());
    aad
}

fn push_u32(output: &mut Vec<u8>, value: usize) -> Result<(), MobileCoreError> {
    let value = u32::try_from(value)
        .map_err(|_| MobileCoreError::Archive("archive field exceeds u32".into()))?;
    output.extend_from_slice(&value.to_be_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn encrypted_archive_round_trips_and_inspects_without_plaintext_leak() {
        let key = RecoveryKey::from_bytes([7; 32]).unwrap();
        let payload = EncryptedArchivePayload {
            archive_kind: "private_node_data".into(),
            source_schema_version: 3,
            canonical_bytes: b"private knowledge payload".repeat(90_000),
        };
        let archive = create_encrypted_archive(&payload, &key).unwrap();
        assert!(!archive
            .windows(b"private knowledge payload".len())
            .any(|window| window == b"private knowledge payload"));
        let inspection = inspect_encrypted_archive(&archive, &key).unwrap();
        assert_eq!(inspection.archive_version, MOBILE_ARCHIVE_VERSION);
        assert!(inspection.chunk_count >= 2);
        assert_eq!(open_encrypted_archive(&archive, &key).unwrap(), payload);
    }

    #[test]
    fn wrong_key_corruption_truncation_and_version_downgrade_fail_closed() {
        let key = RecoveryKey::from_bytes([7; 32]).unwrap();
        let wrong = RecoveryKey::from_bytes([8; 32]).unwrap();
        let payload = EncryptedArchivePayload {
            archive_kind: "private_node_data".into(),
            source_schema_version: 3,
            canonical_bytes: b"secret".to_vec(),
        };
        let archive = create_encrypted_archive(&payload, &key).unwrap();
        assert!(inspect_encrypted_archive(&archive, &wrong).is_err());

        let mut corrupt = archive.clone();
        let last = corrupt.len() - 1;
        corrupt[last] ^= 1;
        assert!(open_encrypted_archive(&corrupt, &key).is_err());
        assert!(open_encrypted_archive(&archive[..archive.len() - 1], &key).is_err());

        let mut old = archive;
        old[8..10].copy_from_slice(&0u16.to_be_bytes());
        assert!(inspect_encrypted_archive(&old, &key).is_err());
    }

    #[test]
    fn file_archive_streams_multiple_chunks_and_activates_only_after_full_verification() {
        let directory = tempdir().unwrap();
        let source = directory.path().join("private.redb");
        let archive = directory.path().join("node.onebrain");
        let restored = directory.path().join("restored.redb");
        let mut bytes = vec![0u8; ARCHIVE_CHUNK_BYTES * 2 + 73];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::try_from(index % 251).unwrap();
        }
        fs::write(&source, &bytes).unwrap();
        let key = RecoveryKey::from_bytes([9; 32]).unwrap();
        let created =
            create_encrypted_archive_file(&source, &archive, "private_node_data", 7, &key).unwrap();
        assert_eq!(created.chunk_count, 3);
        assert_eq!(
            inspect_encrypted_archive_file(&archive, &key).unwrap(),
            created
        );
        let opened = open_encrypted_archive_file(&archive, &restored, &key).unwrap();
        assert_eq!(opened, created);
        assert_eq!(fs::read(&restored).unwrap(), bytes);
        assert!(open_encrypted_archive_file(&archive, &restored, &key).is_err());
    }

    #[test]
    fn corrupt_file_archive_never_renames_partial_restore_into_place() {
        let directory = tempdir().unwrap();
        let source = directory.path().join("private.redb");
        let archive = directory.path().join("node.onebrain");
        let restored = directory.path().join("restored.redb");
        fs::write(&source, b"private state").unwrap();
        let key = RecoveryKey::from_bytes([9; 32]).unwrap();
        create_encrypted_archive_file(&source, &archive, "private_node_data", 7, &key).unwrap();
        let mut corrupt = fs::read(&archive).unwrap();
        let last = corrupt.len() - 1;
        corrupt[last] ^= 1;
        fs::write(&archive, corrupt).unwrap();
        assert!(open_encrypted_archive_file(&archive, &restored, &key).is_err());
        assert!(!restored.exists());
        assert!(!creating_path(&restored).exists());
    }
}
