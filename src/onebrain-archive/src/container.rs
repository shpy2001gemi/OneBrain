use std::io::{Read, Seek, SeekFrom, Write};

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::crypto::{ArchiveCredential, ArchiveCredentialKind, RecoveryKey};
use crate::limits::{ArchiveKdfProfileV1, ArchiveLimits, ARCHIVE_CHUNK_BYTES};
use crate::ArchiveError;

pub const OBAR_V1_MAGIC: &[u8; 8] = b"OBARV001";
pub const OBAR_V2_MAGIC: &[u8; 8] = b"OBARV002";

const V1_VERSION: u16 = 1;
const V2_VERSION: u16 = 2;
const TAG_BYTES: usize = 16;
const SALT_BYTES: usize = 16;
const NONCE_PREFIX_BYTES: usize = 16;
const V2_HEADER_BYTES: usize = 60;
const V2_AAD_CONTEXT: &[u8] = b"onebrain:base:archive-container:2\0";
const V1_AAD_CONTEXT: &[u8] = b"onebrain:mobile:portable-archive:1\0";
const V1_CHUNK_BYTES: usize = 1024 * 1024;
const V1_MAX_MANIFEST_CIPHERTEXT_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchiveInspection {
    pub archive_version: u16,
    pub credential_kind: ArchiveCredentialKind,
    pub plaintext_length: u64,
    pub plaintext_blake3: [u8; 32],
    pub chunk_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyArchiveInspection {
    pub archive_version: u16,
    pub archive_kind: String,
    pub source_schema_version: u32,
    pub payload_length: u64,
    /// Manifest-declared digest. Legacy inspection authenticates metadata but
    /// intentionally does not produce activatable verified dataset state.
    pub declared_payload_blake3: [u8; 32],
    pub chunk_count: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestV2 {
    version: u16,
    plaintext_length: u64,
    plaintext_blake3: String,
    chunk_bytes: u32,
    chunk_count: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyManifestV1 {
    archive_version: u16,
    archive_kind: String,
    source_schema_version: u32,
    payload_length: u64,
    payload_digest: String,
    chunk_bytes: u32,
    chunk_count: u32,
}

#[derive(Clone)]
struct HeaderV2 {
    credential_kind: ArchiveCredentialKind,
    kdf: ArchiveKdfProfileV1,
    salt: [u8; SALT_BYTES],
    nonce_prefix: [u8; NONCE_PREFIX_BYTES],
    manifest_ciphertext_bytes: u32,
}

pub fn seal_archive<R: Read, W: Write>(
    input: R,
    mut output: W,
    credential: &ArchiveCredential,
    limits: &ArchiveLimits,
) -> Result<ArchiveInspection, ArchiveError> {
    limits.validate()?;
    let plaintext = Zeroizing::new(read_bounded(
        input,
        limits.max_total_plaintext_bytes.min(limits.max_entry_bytes),
    )?);
    let chunk_count = plaintext.len().div_ceil(ARCHIVE_CHUNK_BYTES);
    if chunk_count as u64 > limits.max_chunks() {
        return Err(ArchiveError::Limit);
    }
    let plaintext_blake3 = *blake3::hash(&plaintext).as_bytes();
    let manifest = ManifestV2 {
        version: V2_VERSION,
        plaintext_length: plaintext.len() as u64,
        plaintext_blake3: encode_hex(&plaintext_blake3),
        chunk_bytes: ARCHIVE_CHUNK_BYTES as u32,
        chunk_count: u32::try_from(chunk_count).map_err(|_| ArchiveError::Limit)?,
    };
    let manifest_bytes = serde_json::to_vec(&manifest)?;
    if manifest_bytes.len() as u64 > limits.max_manifest_bytes {
        return Err(ArchiveError::Limit);
    }

    let mut salt = [0u8; SALT_BYTES];
    let mut nonce_prefix = [0u8; NONCE_PREFIX_BYTES];
    getrandom::fill(&mut salt).map_err(|_| ArchiveError::InvalidCredential)?;
    getrandom::fill(&mut nonce_prefix).map_err(|_| ArchiveError::InvalidCredential)?;
    let kdf = match credential.kind() {
        ArchiveCredentialKind::Password => ArchiveKdfProfileV1::PASSWORD,
        ArchiveCredentialKind::RecoveryKey => ArchiveKdfProfileV1::RECOVERY_KEY,
    };
    let manifest_ciphertext_bytes = manifest_bytes
        .len()
        .checked_add(TAG_BYTES)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or(ArchiveError::Limit)?;
    let header = HeaderV2 {
        credential_kind: credential.kind(),
        kdf,
        salt,
        nonce_prefix,
        manifest_ciphertext_bytes,
    };
    let header_bytes = header.encode();
    let key = credential.derive_v2(&header.salt, header.kdf)?;
    let cipher = XChaCha20Poly1305::new((&*key).into());
    let encrypted_manifest = encrypt_part(&cipher, &header, 0, &manifest_bytes)?;

    output.write_all(&header_bytes)?;
    output.write_all(&encrypted_manifest)?;
    output.write_all(&manifest.chunk_count.to_be_bytes())?;
    for (index, chunk) in plaintext.chunks(ARCHIVE_CHUNK_BYTES).enumerate() {
        let ciphertext = encrypt_part(&cipher, &header, index as u64 + 1, chunk)?;
        let length = u32::try_from(ciphertext.len()).map_err(|_| ArchiveError::Limit)?;
        output.write_all(&length.to_be_bytes())?;
        output.write_all(&ciphertext)?;
    }
    output.flush()?;
    Ok(ArchiveInspection {
        archive_version: V2_VERSION,
        credential_kind: credential.kind(),
        plaintext_length: manifest.plaintext_length,
        plaintext_blake3,
        chunk_count: manifest.chunk_count,
    })
}

pub fn inspect_legacy_archive_v1<R: Read>(
    mut input: R,
    recovery_key: &RecoveryKey,
    limits: &ArchiveLimits,
) -> Result<LegacyArchiveInspection, ArchiveError> {
    limits.validate()?;
    let mut magic = [0u8; 8];
    input.read_exact(&mut magic)?;
    if &magic != OBAR_V1_MAGIC {
        return Err(ArchiveError::InvalidProfile);
    }
    if read_u16(&mut input)? != V1_VERSION {
        return Err(ArchiveError::InvalidProfile);
    }
    let mut nonce_prefix = [0u8; NONCE_PREFIX_BYTES];
    input.read_exact(&mut nonce_prefix)?;
    let manifest_length = read_u32(&mut input)? as usize;
    if !(TAG_BYTES..=V1_MAX_MANIFEST_CIPHERTEXT_BYTES).contains(&manifest_length)
        || manifest_length as u64 > limits.max_manifest_bytes
    {
        return Err(ArchiveError::Limit);
    }
    let mut consumed = 8u64 + 2 + NONCE_PREFIX_BYTES as u64 + 4 + manifest_length as u64;
    if consumed > limits.max_spool_bytes {
        return Err(ArchiveError::Limit);
    }
    let mut encrypted_manifest = vec![0u8; manifest_length];
    input.read_exact(&mut encrypted_manifest)?;
    let key = recovery_key.legacy_key();
    let cipher = XChaCha20Poly1305::new((&*key).into());
    let manifest_bytes = Zeroizing::new(
        cipher
            .decrypt(
                &nonce(&nonce_prefix, 0),
                Payload {
                    msg: &encrypted_manifest,
                    aad: &legacy_aad(0),
                },
            )
            .map_err(|_| ArchiveError::Authentication)?,
    );
    let manifest: LegacyManifestV1 = serde_json::from_slice(&manifest_bytes)?;
    if serde_json::to_vec(&manifest)?.as_slice() != manifest_bytes.as_slice() {
        return Err(ArchiveError::InvalidProfile);
    }
    validate_legacy_manifest(&manifest, limits)?;
    let encoded_chunk_count = read_u32(&mut input)?;
    consumed = consumed.checked_add(4).ok_or(ArchiveError::Limit)?;
    if encoded_chunk_count != manifest.chunk_count {
        return Err(ArchiveError::Integrity);
    }
    for index in 0..encoded_chunk_count as usize {
        let length = read_u32(&mut input)? as usize;
        let expected = expected_plaintext_chunk_length(
            manifest.payload_length,
            index,
            encoded_chunk_count as usize,
            V1_CHUNK_BYTES,
        )?
        .checked_add(TAG_BYTES)
        .ok_or(ArchiveError::Limit)?;
        if length != expected {
            return Err(ArchiveError::Integrity);
        }
        consumed = consumed
            .checked_add(4 + length as u64)
            .ok_or(ArchiveError::Limit)?;
        if consumed > limits.max_spool_bytes {
            return Err(ArchiveError::Limit);
        }
        drain_exact(&mut input, length)?;
    }
    let mut trailing = [0u8; 1];
    if input.read(&mut trailing)? != 0 {
        return Err(ArchiveError::TrailingBytes);
    }
    Ok(LegacyArchiveInspection {
        archive_version: manifest.archive_version,
        archive_kind: manifest.archive_kind,
        source_schema_version: manifest.source_schema_version,
        payload_length: manifest.payload_length,
        declared_payload_blake3: decode_hex_32(&manifest.payload_digest)?,
        chunk_count: manifest.chunk_count,
    })
}

pub(crate) fn verify_v2<R: Read + Seek>(
    input: &mut R,
    credential: &ArchiveCredential,
    limits: &ArchiveLimits,
) -> Result<ArchiveInspection, ArchiveError> {
    input.seek(SeekFrom::Start(0))?;
    read_v2(input, credential, limits, false).map(|(inspection, _)| inspection)
}

pub(crate) fn open_v2<R: Read + Seek>(
    input: &mut R,
    credential: &ArchiveCredential,
    limits: &ArchiveLimits,
) -> Result<(ArchiveInspection, Zeroizing<Vec<u8>>), ArchiveError> {
    input.seek(SeekFrom::Start(0))?;
    read_v2(input, credential, limits, true)
}

fn read_v2<R: Read>(
    input: &mut R,
    credential: &ArchiveCredential,
    limits: &ArchiveLimits,
    collect_plaintext: bool,
) -> Result<(ArchiveInspection, Zeroizing<Vec<u8>>), ArchiveError> {
    limits.validate()?;
    let header = HeaderV2::read(input, limits)?;
    if header.credential_kind != credential.kind() {
        return Err(ArchiveError::Authentication);
    }
    let key = credential.derive_v2(&header.salt, header.kdf)?;
    let cipher = XChaCha20Poly1305::new((&*key).into());
    let mut encrypted_manifest = vec![0u8; header.manifest_ciphertext_bytes as usize];
    input.read_exact(&mut encrypted_manifest)?;
    let manifest_bytes = Zeroizing::new(
        cipher
            .decrypt(
                &part_nonce(&header, 0),
                Payload {
                    msg: &encrypted_manifest,
                    aad: &part_aad(&header, 0),
                },
            )
            .map_err(|_| ArchiveError::Authentication)?,
    );
    let manifest: ManifestV2 = serde_json::from_slice(&manifest_bytes)?;
    if serde_json::to_vec(&manifest)?.as_slice() != manifest_bytes.as_slice() {
        return Err(ArchiveError::InvalidProfile);
    }
    validate_manifest_v2(&manifest, limits)?;
    let encoded_chunk_count = read_u32(input)?;
    if encoded_chunk_count != manifest.chunk_count {
        return Err(ArchiveError::Integrity);
    }

    let capacity = if collect_plaintext {
        usize::try_from(manifest.plaintext_length).map_err(|_| ArchiveError::Limit)?
    } else {
        0
    };
    let mut plaintext = Zeroizing::new(Vec::with_capacity(capacity));
    let mut hasher = blake3::Hasher::new();
    let mut total = 0u64;
    for index in 0..encoded_chunk_count as usize {
        let ciphertext_length = read_u32(input)? as usize;
        let expected_plaintext = expected_plaintext_chunk_length(
            manifest.plaintext_length,
            index,
            encoded_chunk_count as usize,
            ARCHIVE_CHUNK_BYTES,
        )?;
        if ciphertext_length != expected_plaintext + TAG_BYTES {
            return Err(ArchiveError::Integrity);
        }
        let mut ciphertext = vec![0u8; ciphertext_length];
        input.read_exact(&mut ciphertext)?;
        let part = Zeroizing::new(
            cipher
                .decrypt(
                    &part_nonce(&header, index as u64 + 1),
                    Payload {
                        msg: &ciphertext,
                        aad: &part_aad(&header, index as u64 + 1),
                    },
                )
                .map_err(|_| ArchiveError::Authentication)?,
        );
        if part.len() != expected_plaintext {
            return Err(ArchiveError::Integrity);
        }
        total = total
            .checked_add(part.len() as u64)
            .ok_or(ArchiveError::Limit)?;
        hasher.update(&part);
        if collect_plaintext {
            plaintext.extend_from_slice(&part);
        }
    }
    let mut trailing = [0u8; 1];
    if input.read(&mut trailing)? != 0 {
        return Err(ArchiveError::TrailingBytes);
    }
    let expected_digest = decode_hex_32(&manifest.plaintext_blake3)?;
    let actual_digest = *hasher.finalize().as_bytes();
    if total != manifest.plaintext_length || actual_digest != expected_digest {
        return Err(ArchiveError::Integrity);
    }
    Ok((
        ArchiveInspection {
            archive_version: V2_VERSION,
            credential_kind: header.credential_kind,
            plaintext_length: total,
            plaintext_blake3: actual_digest,
            chunk_count: encoded_chunk_count,
        },
        plaintext,
    ))
}

impl HeaderV2 {
    fn encode(&self) -> [u8; V2_HEADER_BYTES] {
        let mut bytes = [0u8; V2_HEADER_BYTES];
        bytes[..8].copy_from_slice(OBAR_V2_MAGIC);
        bytes[8..10].copy_from_slice(&V2_VERSION.to_be_bytes());
        bytes[10] = self.credential_kind as u8;
        bytes[11] = 0;
        bytes[12..16].copy_from_slice(&self.kdf.memory_kib.to_be_bytes());
        bytes[16..20].copy_from_slice(&self.kdf.iterations.to_be_bytes());
        bytes[20..24].copy_from_slice(&self.kdf.parallelism.to_be_bytes());
        bytes[24..40].copy_from_slice(&self.salt);
        bytes[40..56].copy_from_slice(&self.nonce_prefix);
        bytes[56..60].copy_from_slice(&self.manifest_ciphertext_bytes.to_be_bytes());
        bytes
    }

    fn read(input: &mut impl Read, limits: &ArchiveLimits) -> Result<Self, ArchiveError> {
        let mut bytes = [0u8; V2_HEADER_BYTES];
        input.read_exact(&mut bytes)?;
        if &bytes[..8] != OBAR_V2_MAGIC
            || u16::from_be_bytes([bytes[8], bytes[9]]) != V2_VERSION
            || bytes[11] != 0
        {
            return Err(ArchiveError::InvalidProfile);
        }
        let credential_kind = match bytes[10] {
            1 => ArchiveCredentialKind::Password,
            2 => ArchiveCredentialKind::RecoveryKey,
            _ => return Err(ArchiveError::InvalidProfile),
        };
        let kdf = ArchiveKdfProfileV1 {
            memory_kib: u32::from_be_bytes(
                bytes[12..16]
                    .try_into()
                    .map_err(|_| ArchiveError::Malformed)?,
            ),
            iterations: u32::from_be_bytes(
                bytes[16..20]
                    .try_into()
                    .map_err(|_| ArchiveError::Malformed)?,
            ),
            parallelism: u32::from_be_bytes(
                bytes[20..24]
                    .try_into()
                    .map_err(|_| ArchiveError::Malformed)?,
            ),
        };
        let expected = match credential_kind {
            ArchiveCredentialKind::Password => ArchiveKdfProfileV1::PASSWORD,
            ArchiveCredentialKind::RecoveryKey => ArchiveKdfProfileV1::RECOVERY_KEY,
        };
        if kdf != expected {
            return Err(ArchiveError::InvalidProfile);
        }
        let mut salt = [0u8; SALT_BYTES];
        salt.copy_from_slice(&bytes[24..40]);
        let mut nonce_prefix = [0u8; NONCE_PREFIX_BYTES];
        nonce_prefix.copy_from_slice(&bytes[40..56]);
        let manifest_ciphertext_bytes = u32::from_be_bytes(
            bytes[56..60]
                .try_into()
                .map_err(|_| ArchiveError::Malformed)?,
        );
        if manifest_ciphertext_bytes < TAG_BYTES as u32
            || manifest_ciphertext_bytes as u64 > limits.max_manifest_bytes + TAG_BYTES as u64
        {
            return Err(ArchiveError::Limit);
        }
        Ok(Self {
            credential_kind,
            kdf,
            salt,
            nonce_prefix,
            manifest_ciphertext_bytes,
        })
    }
}

fn validate_manifest_v2(manifest: &ManifestV2, limits: &ArchiveLimits) -> Result<(), ArchiveError> {
    if manifest.version != V2_VERSION || manifest.chunk_bytes != ARCHIVE_CHUNK_BYTES as u32 {
        return Err(ArchiveError::InvalidProfile);
    }
    if manifest.plaintext_length > limits.max_total_plaintext_bytes
        || manifest.plaintext_length > limits.max_entry_bytes
        || manifest.chunk_count as u64 > limits.max_chunks()
        || manifest.chunk_count as u64
            != manifest
                .plaintext_length
                .div_ceil(ARCHIVE_CHUNK_BYTES as u64)
    {
        return Err(ArchiveError::Limit);
    }
    decode_hex_32(&manifest.plaintext_blake3)?;
    Ok(())
}

fn validate_legacy_manifest(
    manifest: &LegacyManifestV1,
    limits: &ArchiveLimits,
) -> Result<(), ArchiveError> {
    if manifest.archive_version != V1_VERSION
        || manifest.chunk_bytes != V1_CHUNK_BYTES as u32
        || manifest.archive_kind.is_empty()
        || manifest.archive_kind.len() > 64
        || !manifest
            .archive_kind
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(ArchiveError::InvalidProfile);
    }
    if manifest.payload_length > limits.max_total_plaintext_bytes
        || manifest.payload_length > limits.max_entry_bytes
        || manifest.chunk_count as u64 > limits.max_chunks()
        || manifest.chunk_count as u64 != manifest.payload_length.div_ceil(V1_CHUNK_BYTES as u64)
    {
        return Err(ArchiveError::Limit);
    }
    decode_hex_32(&manifest.payload_digest)?;
    Ok(())
}

fn encrypt_part(
    cipher: &XChaCha20Poly1305,
    header: &HeaderV2,
    index: u64,
    plaintext: &[u8],
) -> Result<Vec<u8>, ArchiveError> {
    cipher
        .encrypt(
            &part_nonce(header, index),
            Payload {
                msg: plaintext,
                aad: &part_aad(header, index),
            },
        )
        .map_err(|_| ArchiveError::Authentication)
}

fn part_nonce(header: &HeaderV2, index: u64) -> XNonce {
    nonce(&header.nonce_prefix, index)
}

fn nonce(prefix: &[u8; 16], index: u64) -> XNonce {
    let mut bytes = [0u8; 24];
    bytes[..16].copy_from_slice(prefix);
    bytes[16..].copy_from_slice(&index.to_be_bytes());
    bytes.into()
}

fn part_aad(header: &HeaderV2, index: u64) -> Vec<u8> {
    let encoded = header.encode();
    let mut aad = Vec::with_capacity(V2_AAD_CONTEXT.len() + encoded.len() + 8);
    aad.extend_from_slice(V2_AAD_CONTEXT);
    aad.extend_from_slice(&encoded);
    aad.extend_from_slice(&index.to_be_bytes());
    aad
}

fn legacy_aad(index: u64) -> Vec<u8> {
    let mut aad = Vec::with_capacity(V1_AAD_CONTEXT.len() + 10);
    aad.extend_from_slice(V1_AAD_CONTEXT);
    aad.extend_from_slice(&V1_VERSION.to_be_bytes());
    aad.extend_from_slice(&index.to_be_bytes());
    aad
}

fn read_bounded(input: impl Read, max: u64) -> Result<Vec<u8>, ArchiveError> {
    let read_limit = max.checked_add(1).ok_or(ArchiveError::Limit)?;
    let mut bytes = Vec::new();
    input.take(read_limit).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(ArchiveError::Limit);
    }
    Ok(bytes)
}

fn expected_plaintext_chunk_length(
    total: u64,
    index: usize,
    chunk_count: usize,
    chunk_bytes: usize,
) -> Result<usize, ArchiveError> {
    if index >= chunk_count {
        return Err(ArchiveError::Integrity);
    }
    let offset = (index as u64)
        .checked_mul(chunk_bytes as u64)
        .ok_or(ArchiveError::Limit)?;
    usize::try_from((total - offset).min(chunk_bytes as u64)).map_err(|_| ArchiveError::Limit)
}

fn read_u32(input: &mut impl Read) -> Result<u32, ArchiveError> {
    let mut bytes = [0u8; 4];
    input.read_exact(&mut bytes)?;
    Ok(u32::from_be_bytes(bytes))
}

fn read_u16(input: &mut impl Read) -> Result<u16, ArchiveError> {
    let mut bytes = [0u8; 2];
    input.read_exact(&mut bytes)?;
    Ok(u16::from_be_bytes(bytes))
}

fn drain_exact(input: &mut impl Read, length: usize) -> Result<(), ArchiveError> {
    let mut remaining = length;
    let mut buffer = [0u8; 64 * 1024];
    while remaining > 0 {
        let read = remaining.min(buffer.len());
        input.read_exact(&mut buffer[..read])?;
        remaining -= read;
    }
    Ok(())
}

fn encode_hex(bytes: &[u8; 32]) -> String {
    let mut result = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(result, "{byte:02x}");
    }
    result
}

fn decode_hex_32(value: &str) -> Result<[u8; 32], ArchiveError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ArchiveError::Malformed);
    }
    let mut output = [0u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| ArchiveError::Malformed)?;
    }
    Ok(output)
}
