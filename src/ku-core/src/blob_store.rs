//! Blob Store — Core types for media/file attachment storage.
//!
//! Blobs are stored separately from KU Core DNA. A KU references
//! a blob via `MediaRef { system: 0x01, id: OB-CID }` in its instructions.
//!
//! OB-CID format: [version:u8][type:u8][blake3:32B] = 34 bytes

use serde::{Deserialize, Serialize};

/// Chunk size: 256KB (IPFS-compatible)
pub const BLOB_CHUNK_SIZE: usize = 256 * 1024;

/// Max single blob size: 100MB
pub const BLOB_MAX_SIZE: u64 = 100 * 1024 * 1024;

/// Max blobs per KU
pub const BLOB_MAX_PER_KU: usize = 10;

/// MediaRef system byte for OBS Blob Store
pub const BLOB_MEDIAREF_SYSTEM: u8 = 0x01;

/// OB-CID version
pub const BLOB_CID_VERSION: u8 = 0x01;

// ── Blob Type ──────────────────────────────────────────────────────────────

/// Type of blob content, inferred from file extension or magic bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum BlobType {
    /// Generic binary blob.
    Raw = 0x00,
    /// Image (JPEG, PNG, WebP, GIF, BMP, SVG).
    Image = 0x01,
    /// Video (MP4, WebM, MKV, AVI, MOV).
    Video = 0x02,
    /// Audio (MP3, OGG, FLAC, WAV, M4A).
    Audio = 0x03,
    /// Document (PDF, DOCX, XLSX, TXT, MD, CSV).
    Document = 0x04,
}

impl BlobType {
    /// Detect blob type from file extension.
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "svg" | "ico" | "tiff" => {
                BlobType::Image
            }
            "mp4" | "webm" | "mkv" | "avi" | "mov" | "wmv" | "flv" => BlobType::Video,
            "mp3" | "ogg" | "flac" | "wav" | "m4a" | "aac" | "wma" => BlobType::Audio,
            "pdf" | "docx" | "xlsx" | "pptx" | "txt" | "md" | "csv" | "json" | "xml" | "html"
            | "rtf" => BlobType::Document,
            _ => BlobType::Raw,
        }
    }

    /// Detect blob type from magic bytes (first 8 bytes).
    pub fn from_magic(bytes: &[u8]) -> Self {
        if bytes.len() < 2 {
            return BlobType::Raw;
        }
        // Image
        if bytes.starts_with(&[0xFF, 0xD8]) {
            return BlobType::Image;
        } // JPEG
        if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
            return BlobType::Image;
        } // PNG
        if bytes.starts_with(b"GIF8") {
            return BlobType::Image;
        } // GIF
        if bytes.starts_with(b"RIFF") && bytes.len() >= 12 && &bytes[8..12] == b"WEBP" {
            return BlobType::Image; // WebP
        }
        // Video (MP4/MOV: ftyp box)
        if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" {
            return BlobType::Video;
        }
        // Audio
        if bytes.starts_with(&[0x49, 0x44, 0x33]) {
            return BlobType::Audio;
        } // MP3 ID3
        if bytes.starts_with(b"OggS") {
            return BlobType::Audio;
        } // OGG
        if bytes.starts_with(b"fLaC") {
            return BlobType::Audio;
        } // FLAC
        if bytes.starts_with(b"RIFF") && bytes.len() >= 12 && &bytes[8..12] == b"WAVE" {
            return BlobType::Audio; // WAV
        }
        // Document
        if bytes.starts_with(&[0x25, 0x50, 0x44, 0x46]) {
            return BlobType::Document;
        } // PDF
        if bytes.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
            return BlobType::Document;
        } // ZIP/DOCX/XLSX

        BlobType::Raw
    }

    /// Detect from both extension and magic bytes, preferring magic.
    pub fn detect(extension: Option<&str>, magic_bytes: &[u8]) -> Self {
        let from_magic = Self::from_magic(magic_bytes);
        if from_magic != BlobType::Raw {
            return from_magic;
        }
        extension.map(Self::from_extension).unwrap_or(BlobType::Raw)
    }

    /// Human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            BlobType::Raw => "Raw",
            BlobType::Image => "Image",
            BlobType::Video => "Video",
            BlobType::Audio => "Audio",
            BlobType::Document => "Document",
        }
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            0x01 => BlobType::Image,
            0x02 => BlobType::Video,
            0x03 => BlobType::Audio,
            0x04 => BlobType::Document,
            _ => BlobType::Raw,
        }
    }
}

// ── Blob CID (OB-CID) ─────────────────────────────────────────────────────

/// 34-byte content-addressed Blob CID.
///
/// Format: `[version:u8][type:u8][blake3:32B]`
///
/// Distinct from KU CIDs (32 bytes) by the 2-byte prefix.
///
/// Serializes as a hex string (68 chars) for JSON compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlobCid(pub [u8; 34]);

impl Serialize for BlobCid {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for BlobCid {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let hex = String::deserialize(deserializer)?;
        BlobCid::from_hex(&hex).ok_or_else(|| serde::de::Error::custom("invalid BlobCid hex"))
    }
}

impl BlobCid {
    /// Create a new BlobCid from file content.
    pub fn from_content(blob_type: BlobType, data: &[u8]) -> Self {
        let hash = blake3::hash(data);
        let mut cid = [0u8; 34];
        cid[0] = BLOB_CID_VERSION;
        cid[1] = blob_type as u8;
        cid[2..34].copy_from_slice(hash.as_bytes());
        BlobCid(cid)
    }

    /// Get the version byte.
    pub fn version(&self) -> u8 {
        self.0[0]
    }

    /// Get the blob type byte.
    pub fn blob_type(&self) -> BlobType {
        BlobType::from_u8(self.0[1])
    }

    /// Get the 32-byte BLAKE3 hash.
    pub fn blake3_hash(&self) -> &[u8; 32] {
        self.0[2..34].try_into().unwrap()
    }

    /// Hex representation of the full 34-byte CID.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Parse from hex string (68 hex chars = 34 bytes).
    pub fn from_hex(hex: &str) -> Option<Self> {
        if hex.len() < 68 {
            return None;
        }
        let bytes: Result<Vec<u8>, _> = (0..34)
            .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16))
            .collect();
        bytes.ok().and_then(|b| {
            let arr: [u8; 34] = b.try_into().ok()?;
            Some(BlobCid(arr))
        })
    }

    /// Short hex for display (first 8 hex chars).
    pub fn short_hex(&self) -> String {
        self.to_hex()[..8].to_string()
    }

    /// Convert to bytes for MediaRef instruction.
    pub fn as_media_ref_id(&self) -> Vec<u8> {
        self.0.to_vec()
    }
}

impl std::fmt::Display for BlobCid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

// ── Blob Metadata ─────────────────────────────────────────────────────────

/// Metadata about a stored blob.
///
/// Stored as JSON in the `blob_meta` redb table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobMeta {
    /// 34-byte OB-CID (hex-encoded for JSON).
    pub blob_cid_hex: String,
    /// Original filename.
    pub original_name: String,
    /// MIME type (e.g., "image/jpeg").
    pub mime_type: String,
    /// Total file size in bytes.
    pub total_size: u64,
    /// Number of chunks.
    pub chunk_count: u32,
    /// Chunk size used (typically 262144 = 256KB).
    pub chunk_size: u32,
    /// Content type.
    pub blob_type: u8,
    /// Creation timestamp (epoch seconds).
    pub created_at: u64,
    /// BLAKE3 hash of entire file (hex, 64 chars).
    pub blake3_hex: String,
    /// KU CIDs that reference this blob (hex-encoded).
    pub referencing_kus: Vec<String>,
    /// Whether this blob is pinned (exempt from GC).
    pub pinned: bool,
    /// Where chunks are stored: "redb" (inline) or "filesystem" (spilled).
    /// Defaults to "redb" for backwards compatibility.
    #[serde(default = "default_storage_mode")]
    pub storage_mode: String,
}

fn default_storage_mode() -> String {
    "redb".to_string()
}

impl BlobMeta {
    /// Check if this blob is orphaned (zero references, not pinned).
    pub fn is_orphaned(&self) -> bool {
        self.referencing_kus.is_empty() && !self.pinned
    }
}

// ── MIME Type Detection ───────────────────────────────────────────────────

/// Infer MIME type from file extension.
pub fn mime_from_extension(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        // Image
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        // Video
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        // Audio
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "m4a" => "audio/mp4",
        // Document
        "pdf" => "application/pdf",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "csv" => "text/csv",
        "json" => "application/json",
        "xml" => "application/xml",
        "html" => "text/html",
        _ => "application/octet-stream",
    }
}

// ── Device Quota ──────────────────────────────────────────────────────────

const GB: u64 = 1024 * 1024 * 1024;

/// Default blob storage quota based on available disk space.
///
/// | Available | Quota | Tier |
/// |-----------|-------|------|
/// | > 500 GB  | 200 GB | Server |
/// | > 200 GB  |  50 GB | Desktop |
/// | > 50 GB   |  20 GB | Laptop |
/// | > 15 GB   |  10 GB | Mobile |
/// | ≤ 15 GB   |   2 GB | IoT |
///
/// Minimum: 10 GB (except IoT).
pub fn default_blob_quota_bytes(available_disk_bytes: u64) -> u64 {
    let quota = if available_disk_bytes > 500 * GB {
        200 * GB // Server
    } else if available_disk_bytes > 200 * GB {
        50 * GB // Desktop
    } else if available_disk_bytes > 50 * GB {
        20 * GB // Laptop
    } else if available_disk_bytes > 15 * GB {
        10 * GB // Mobile (min)
    } else {
        2 * GB // IoT
    };
    // Minimum 10GB except for IoT tier
    if available_disk_bytes > 15 * GB {
        quota.max(10 * GB)
    } else {
        quota
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_cid_roundtrip() {
        let data = b"Hello, this is test content for blob CID";
        let cid = BlobCid::from_content(BlobType::Document, data);
        assert_eq!(cid.version(), BLOB_CID_VERSION);
        assert_eq!(cid.blob_type(), BlobType::Document);

        let hex = cid.to_hex();
        assert_eq!(hex.len(), 68); // 34 bytes × 2

        let parsed = BlobCid::from_hex(&hex).unwrap();
        assert_eq!(cid, parsed);
    }

    #[test]
    fn blob_cid_dedup() {
        let data = b"identical content";
        let cid1 = BlobCid::from_content(BlobType::Raw, data);
        let cid2 = BlobCid::from_content(BlobType::Raw, data);
        assert_eq!(cid1, cid2); // Same content = same CID
    }

    #[test]
    fn blob_type_from_extension() {
        assert_eq!(BlobType::from_extension("jpg"), BlobType::Image);
        assert_eq!(BlobType::from_extension("JPEG"), BlobType::Image);
        assert_eq!(BlobType::from_extension("mp4"), BlobType::Video);
        assert_eq!(BlobType::from_extension("mp3"), BlobType::Audio);
        assert_eq!(BlobType::from_extension("pdf"), BlobType::Document);
        assert_eq!(BlobType::from_extension("xyz"), BlobType::Raw);
    }

    #[test]
    fn blob_type_from_magic() {
        assert_eq!(BlobType::from_magic(&[0xFF, 0xD8, 0xFF]), BlobType::Image); // JPEG
        assert_eq!(
            BlobType::from_magic(&[0x89, 0x50, 0x4E, 0x47]),
            BlobType::Image
        ); // PNG
        assert_eq!(
            BlobType::from_magic(&[0x25, 0x50, 0x44, 0x46]),
            BlobType::Document
        ); // PDF
        assert_eq!(BlobType::from_magic(&[0x00]), BlobType::Raw);
    }

    #[test]
    fn blob_type_detect_prefers_magic() {
        // Magic says Image (JPEG), extension says document
        let magic = &[0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(BlobType::detect(Some("pdf"), magic), BlobType::Image);
    }

    #[test]
    fn default_quota_tiers() {
        assert_eq!(default_blob_quota_bytes(1000 * GB), 200 * GB); // Server
        assert_eq!(default_blob_quota_bytes(300 * GB), 50 * GB); // Desktop
        assert_eq!(default_blob_quota_bytes(100 * GB), 20 * GB); // Laptop
        assert_eq!(default_blob_quota_bytes(20 * GB), 10 * GB); // Mobile
        assert_eq!(default_blob_quota_bytes(5 * GB), 2 * GB); // IoT
    }

    #[test]
    fn blob_meta_orphan_detection() {
        let meta = BlobMeta {
            blob_cid_hex: "01010000".repeat(4) + &"00".repeat(2),
            original_name: "test.jpg".into(),
            mime_type: "image/jpeg".into(),
            total_size: 1024,
            chunk_count: 1,
            chunk_size: 262144,
            blob_type: 0x01,
            created_at: 0,
            blake3_hex: "00".repeat(32),
            referencing_kus: vec![],
            pinned: false,
            storage_mode: "redb".into(),
        };
        assert!(meta.is_orphaned());

        let pinned_meta = BlobMeta {
            pinned: true,
            ..meta.clone()
        };
        assert!(!pinned_meta.is_orphaned());
    }

    #[test]
    fn mime_detection() {
        assert_eq!(mime_from_extension("jpg"), "image/jpeg");
        assert_eq!(mime_from_extension("pdf"), "application/pdf");
        assert_eq!(mime_from_extension("unknown"), "application/octet-stream");
    }
}
