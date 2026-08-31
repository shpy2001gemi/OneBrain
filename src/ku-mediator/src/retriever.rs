//! Deterministic, rebuildable keyword projection over durable private sources.

use ku_core::foundation::{
    BoundedUtf8, LocalSourceTextRecordV1, ObjectCid, ObjectReference, VaultSourceSnapshotRecord,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::path::Path;
use thiserror::Error;

pub const RETRIEVER_INDEX_PROFILE: &str = "onebrain/retriever-index/2";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievedKU {
    pub cid: String,
    pub expression: String,
    pub score: f32,
    pub source: RetrievalSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RetrievalSource {
    Embedding,
    Keyword,
    GraphTraversal,
}

#[derive(Debug, Clone)]
pub struct RetrieverConfig {
    pub top_k: usize,
    pub min_score: f32,
}

impl Default for RetrieverConfig {
    fn default() -> Self {
        Self {
            top_k: 5,
            min_score: 0.3,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetrieverSourceRecord {
    pub subject: ObjectReference,
    pub source_text: String,
    pub source_digest: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetrieverIndexEntryV1 {
    pub subject: ObjectReference,
    pub source_record: ObjectCid,
    pub source_digest: [u8; 32],
    pub expression: BoundedUtf8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetrieverIndexEnvelope {
    pub profile: String,
    pub source_root: [u8; 32],
    pub entries_root: [u8; 32],
    pub entries: Vec<RetrieverIndexEntryV1>,
}

#[derive(Debug, Error)]
pub enum RetrieverError {
    #[error("retriever I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("retriever JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unknown retriever profile")]
    UnknownProfile,
    #[error("duplicate retriever subject")]
    DuplicateSubject,
    #[error("retriever source root mismatch")]
    SourceRootMismatch,
    #[error("retriever entry root mismatch")]
    EntriesRootMismatch,
    #[error("retriever source digest mismatch")]
    SourceDigestMismatch,
    #[error("retriever field is malformed")]
    Malformed,
    #[error("retriever source text is invalid: {0}")]
    SourceText(String),
}

#[derive(Clone)]
pub struct KuRetriever {
    config: RetrieverConfig,
    entries: BTreeMap<(u64, [u8; 32]), RetrieverIndexEntryV1>,
    legacy_read_only: BTreeMap<String, String>,
}

impl KuRetriever {
    pub fn new(config: RetrieverConfig) -> Self {
        Self {
            config,
            entries: BTreeMap::new(),
            legacy_read_only: BTreeMap::new(),
        }
    }

    pub fn upsert_source(&mut self, subject: ObjectReference, expression: String) {
        let Ok(record) = LocalSourceTextRecordV1::new(subject.clone(), expression) else {
            return;
        };
        let Ok((_, source_record)) = record.encode() else {
            return;
        };
        self.entries.insert(
            subject_key(&subject),
            RetrieverIndexEntryV1 {
                subject,
                source_record,
                source_digest: record.source_digest,
                expression: record.source_text,
            },
        );
    }

    pub fn upsert_vault_record(&mut self, record: VaultSourceSnapshotRecord) {
        self.entries.insert(
            subject_key(&record.subject),
            RetrieverIndexEntryV1 {
                subject: record.subject,
                source_record: record.source_record,
                source_digest: record.source_digest,
                expression: record.source_text,
            },
        );
    }

    pub fn remove_source(&mut self, subject: &ObjectReference) -> bool {
        self.entries.remove(&subject_key(subject)).is_some()
    }

    pub fn subjects(&self) -> Vec<ObjectReference> {
        self.entries
            .values()
            .map(|entry| entry.subject.clone())
            .collect()
    }

    /// Compatibility-only legacy evidence. Invalid/non-CID identifiers never
    /// enter the typed Base projection.
    pub fn index_ku(&mut self, cid: String, expression: String) {
        if let Some(cid_bytes) = decode_hex_32(&cid) {
            self.upsert_source(ObjectReference::new(0, cid_bytes), expression);
        } else {
            self.legacy_read_only.insert(cid, expression);
        }
    }

    pub fn get_expression(&self, cid: &str) -> Option<String> {
        decode_hex_32(cid)
            .and_then(|cid| {
                self.entries
                    .values()
                    .find(|entry| entry.subject.cid == cid)
                    .map(|entry| entry.expression.as_str().to_owned())
            })
            .or_else(|| self.legacy_read_only.get(cid).cloned())
    }

    pub fn retrieve(&self, query: &str) -> Vec<RetrievedKU> {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();
        let typed = self.entries.values().map(|entry| {
            (
                encode_hex(&entry.subject.cid),
                entry.expression.as_str().to_owned(),
            )
        });
        let legacy = self
            .legacy_read_only
            .iter()
            .map(|(cid, expression)| (cid.clone(), expression.clone()));
        let mut results = typed
            .chain(legacy)
            .filter_map(|(cid, expression)| {
                let expression_lower = expression.to_lowercase();
                let matching_words = query_words
                    .iter()
                    .filter(|word| word.len() > 2 && expression_lower.contains(*word))
                    .count();
                if matching_words == 0 {
                    return None;
                }
                let score = matching_words as f32 / query_words.len().max(1) as f32;
                (score >= self.config.min_score).then_some(RetrievedKU {
                    cid,
                    expression,
                    score,
                    source: RetrievalSource::Keyword,
                })
            })
            .collect::<Vec<_>>();
        results.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.cid.cmp(&right.cid))
        });
        results.truncate(self.config.top_k);
        results
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.legacy_read_only.clear();
    }

    pub fn index_size(&self) -> usize {
        self.entries.len() + self.legacy_read_only.len()
    }

    pub fn envelope(&self, source_root: [u8; 32]) -> RetrieverIndexEnvelope {
        let entries = self.entries.values().cloned().collect::<Vec<_>>();
        RetrieverIndexEnvelope {
            profile: RETRIEVER_INDEX_PROFILE.to_owned(),
            source_root,
            entries_root: entries_root(&entries),
            entries,
        }
    }

    pub fn save_atomic(&self, path: &Path, source_root: [u8; 32]) -> Result<(), RetrieverError> {
        let parent = path.parent().ok_or(RetrieverError::Malformed)?;
        std::fs::create_dir_all(parent)?;
        let envelope = self.envelope(source_root);
        let bytes = serde_json::to_vec_pretty(&WireEnvelope::from_envelope(&envelope))?;
        let temp = path.with_extension("tmp");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        let result = (|| -> Result<(), RetrieverError> {
            file.write_all(&bytes)?;
            file.sync_all()?;
            std::fs::rename(&temp, path)?;
            sync_parent(parent)?;
            Ok(())
        })();
        if result.is_err() {
            drop(file);
            let _ = std::fs::remove_file(&temp);
        }
        result
    }

    pub fn load_envelope(path: &Path) -> Result<RetrieverIndexEnvelope, RetrieverError> {
        let bytes = std::fs::read(path)?;
        let wire: WireEnvelope = serde_json::from_slice(&bytes)?;
        wire.into_envelope()
    }

    pub fn load_for_source_root(
        path: &Path,
        expected_source_root: [u8; 32],
    ) -> Result<RetrieverIndexEnvelope, RetrieverError> {
        let envelope = Self::load_envelope(path)?;
        if envelope.source_root != expected_source_root {
            return Err(RetrieverError::SourceRootMismatch);
        }
        Ok(envelope)
    }

    pub fn from_envelope(
        envelope: RetrieverIndexEnvelope,
        config: RetrieverConfig,
    ) -> Result<Self, RetrieverError> {
        validate_entries(&envelope.entries)?;
        let entries = envelope
            .entries
            .into_iter()
            .map(|entry| (subject_key(&entry.subject), entry))
            .collect();
        Ok(Self {
            config,
            entries,
            legacy_read_only: BTreeMap::new(),
        })
    }

    pub fn from_vault_records(
        records: Vec<VaultSourceSnapshotRecord>,
        config: RetrieverConfig,
    ) -> Result<Self, RetrieverError> {
        let mut retriever = Self::new(config);
        for record in records {
            if retriever
                .entries
                .contains_key(&subject_key(&record.subject))
            {
                return Err(RetrieverError::DuplicateSubject);
            }
            retriever.upsert_vault_record(record);
        }
        Ok(retriever)
    }

    pub fn save(&self, path: &Path) -> Result<(), io::Error> {
        self.save_atomic(path, self.compatibility_source_root())
            .map_err(retriever_io_error)
    }

    pub fn load(path: &Path) -> Result<Self, io::Error> {
        Self::load_with_config(path, RetrieverConfig::default())
    }

    pub fn load_with_config(path: &Path, config: RetrieverConfig) -> Result<Self, io::Error> {
        if !path.exists() {
            return Ok(Self::new(config));
        }
        if let Ok(envelope) = Self::load_envelope(path) {
            return Self::from_envelope(envelope, config).map_err(retriever_io_error);
        }
        let data = std::fs::read_to_string(path)?;
        let legacy: Vec<(String, String)> = serde_json::from_str(&data)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let mut retriever = Self::new(config);
        for (cid, expression) in legacy {
            retriever.index_ku(cid, expression);
        }
        Ok(retriever)
    }

    fn compatibility_source_root(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new_derive_key("onebrain:retriever:compat-root:1");
        hasher.update(&entries_root(
            &self.entries.values().cloned().collect::<Vec<_>>(),
        ));
        *hasher.finalize().as_bytes()
    }
}

impl Default for KuRetriever {
    fn default() -> Self {
        Self::new(RetrieverConfig::default())
    }
}

pub fn retriever_source_root(
    accepted_vnext_root: [u8; 32],
    vault_source_root: [u8; 32],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key("onebrain:retriever:source-root:2");
    hasher.update(blake3::hash(RETRIEVER_INDEX_PROFILE.as_bytes()).as_bytes());
    hasher.update(&accepted_vnext_root);
    hasher.update(&vault_source_root);
    *hasher.finalize().as_bytes()
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireEnvelope {
    profile: String,
    source_root: String,
    entries_root: String,
    entries: Vec<WireEntry>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireEntry {
    reference_kind: u64,
    subject_cid: String,
    source_record: String,
    source_digest: String,
    expression: String,
}

impl WireEnvelope {
    fn from_envelope(envelope: &RetrieverIndexEnvelope) -> Self {
        Self {
            profile: envelope.profile.clone(),
            source_root: encode_hex(&envelope.source_root),
            entries_root: encode_hex(&envelope.entries_root),
            entries: envelope
                .entries
                .iter()
                .map(|entry| WireEntry {
                    reference_kind: entry.subject.reference_kind,
                    subject_cid: encode_hex(&entry.subject.cid),
                    source_record: encode_hex(entry.source_record.as_bytes()),
                    source_digest: encode_hex(&entry.source_digest),
                    expression: entry.expression.as_str().to_owned(),
                })
                .collect(),
        }
    }

    fn into_envelope(self) -> Result<RetrieverIndexEnvelope, RetrieverError> {
        if self.profile != RETRIEVER_INDEX_PROFILE {
            return Err(RetrieverError::UnknownProfile);
        }
        let source_root = decode_hex_32(&self.source_root).ok_or(RetrieverError::Malformed)?;
        let declared_entries_root =
            decode_hex_32(&self.entries_root).ok_or(RetrieverError::Malformed)?;
        let entries = self
            .entries
            .into_iter()
            .map(|entry| {
                let subject = ObjectReference::new(
                    entry.reference_kind,
                    decode_hex_32(&entry.subject_cid).ok_or(RetrieverError::Malformed)?,
                );
                let expression = BoundedUtf8::new(entry.expression)
                    .map_err(|error| RetrieverError::SourceText(error.to_string()))?;
                let source_digest =
                    decode_hex_32(&entry.source_digest).ok_or(RetrieverError::Malformed)?;
                let source_record = ObjectCid::from_bytes(
                    decode_hex_32(&entry.source_record).ok_or(RetrieverError::Malformed)?,
                );
                let local =
                    LocalSourceTextRecordV1::new(subject.clone(), expression.as_str().to_owned())
                        .map_err(|error| RetrieverError::SourceText(error.to_string()))?;
                if local.source_digest != source_digest {
                    return Err(RetrieverError::SourceDigestMismatch);
                }
                let (_, computed_source_record) = local
                    .encode()
                    .map_err(|error| RetrieverError::SourceText(error.to_string()))?;
                if computed_source_record != source_record {
                    return Err(RetrieverError::SourceDigestMismatch);
                }
                Ok(RetrieverIndexEntryV1 {
                    subject,
                    source_record,
                    source_digest,
                    expression,
                })
            })
            .collect::<Result<Vec<_>, RetrieverError>>()?;
        validate_entries(&entries)?;
        if entries_root(&entries) != declared_entries_root {
            return Err(RetrieverError::EntriesRootMismatch);
        }
        Ok(RetrieverIndexEnvelope {
            profile: self.profile,
            source_root,
            entries_root: declared_entries_root,
            entries,
        })
    }
}

fn validate_entries(entries: &[RetrieverIndexEntryV1]) -> Result<(), RetrieverError> {
    let mut seen = BTreeSet::new();
    for entry in entries {
        if !seen.insert(subject_key(&entry.subject)) {
            return Err(RetrieverError::DuplicateSubject);
        }
    }
    Ok(())
}

fn entries_root(entries: &[RetrieverIndexEntryV1]) -> [u8; 32] {
    let mut sorted = entries.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|entry| subject_key(&entry.subject));
    let mut hasher = blake3::Hasher::new_derive_key("onebrain:retriever:entries-root:2");
    for entry in sorted {
        hasher.update(&entry.subject.reference_kind.to_be_bytes());
        hasher.update(&entry.subject.cid);
        hasher.update(entry.source_record.as_bytes());
        hasher.update(&entry.source_digest);
        hasher.update(&(entry.expression.as_str().len() as u64).to_be_bytes());
        hasher.update(entry.expression.as_str().as_bytes());
    }
    *hasher.finalize().as_bytes()
}

fn subject_key(reference: &ObjectReference) -> (u64, [u8; 32]) {
    (reference.reference_kind, reference.cid)
}

fn sync_parent(parent: &Path) -> io::Result<()> {
    match std::fs::File::open(parent) {
        Ok(directory) => directory.sync_all(),
        Err(error) if cfg!(windows) && error.kind() == io::ErrorKind::PermissionDenied => Ok(()),
        Err(error) => Err(error),
    }
}

fn retriever_io_error(error: RetrieverError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

fn encode_hex(bytes: &[u8; 32]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn decode_hex_32(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }
    let mut decoded = [0; 32];
    for (index, byte) in decoded.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn populated_retriever() -> KuRetriever {
        let mut retriever = KuRetriever::default();
        retriever.index_ku("cid1".into(), "Water boils at 100 degrees Celsius".into());
        retriever.index_ku(
            "cid2".into(),
            "The sky is blue due to Rayleigh scattering".into(),
        );
        retriever.index_ku("cid3".into(), "Water freezes at zero degrees".into());
        retriever.index_ku(
            "cid4".into(),
            "Rust is a systems programming language".into(),
        );
        retriever
    }

    #[test]
    fn test_retrieve_keyword_match_and_scoring() {
        let retriever = populated_retriever();
        let results = retriever.retrieve("water temperature degrees");
        assert!(results.iter().any(|result| result.cid == "cid1"));
        assert!(results
            .windows(2)
            .all(|pair| pair[0].score >= pair[1].score));
    }

    #[test]
    fn test_retrieve_no_match() {
        assert!(populated_retriever()
            .retrieve("quantum entanglement")
            .is_empty());
    }

    #[test]
    fn test_top_k_and_min_score() {
        let mut retriever = KuRetriever::new(RetrieverConfig {
            top_k: 1,
            min_score: 0.9,
        });
        retriever.index_ku("a".into(), "water boils".into());
        assert!(retriever
            .retrieve("water and other things and more stuff")
            .is_empty());
    }

    #[test]
    fn upsert_is_unique_and_remove_clears_complete_reference() {
        let subject = ObjectReference::new(9, [7; 32]);
        let mut retriever = KuRetriever::default();
        retriever.upsert_source(subject.clone(), "first".into());
        retriever.upsert_source(subject.clone(), "second".into());
        assert_eq!(retriever.index_size(), 1);
        assert_eq!(retriever.subjects(), vec![subject.clone()]);
        assert!(retriever.remove_source(&subject));
        assert_eq!(retriever.index_size(), 0);
    }

    #[test]
    fn envelope_round_trip_and_source_root_gate() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("retriever.json");
        let subject = ObjectReference::new(1, [3; 32]);
        let mut retriever = KuRetriever::default();
        retriever.upsert_source(subject.clone(), "nguồn chính xác".into());
        retriever.save_atomic(&path, [8; 32]).unwrap();
        let envelope = KuRetriever::load_for_source_root(&path, [8; 32]).unwrap();
        assert_eq!(envelope.entries[0].subject, subject);
        assert!(matches!(
            KuRetriever::load_for_source_root(&path, [9; 32]),
            Err(RetrieverError::SourceRootMismatch)
        ));
    }

    #[test]
    fn envelope_rejects_unknown_profile_duplicate_and_root_corruption() {
        let subject = ObjectReference::new(1, [4; 32]);
        let mut retriever = KuRetriever::default();
        retriever.upsert_source(subject, "exact".into());
        let envelope = retriever.envelope([2; 32]);
        let mut wire = WireEnvelope::from_envelope(&envelope);
        wire.profile = "unknown".into();
        assert!(matches!(
            wire.into_envelope(),
            Err(RetrieverError::UnknownProfile)
        ));

        let mut wire = WireEnvelope::from_envelope(&envelope);
        wire.entries.push(WireEntry {
            reference_kind: wire.entries[0].reference_kind,
            subject_cid: wire.entries[0].subject_cid.clone(),
            source_record: wire.entries[0].source_record.clone(),
            source_digest: wire.entries[0].source_digest.clone(),
            expression: wire.entries[0].expression.clone(),
        });
        assert!(matches!(
            wire.into_envelope(),
            Err(RetrieverError::DuplicateSubject)
        ));

        let mut wire = WireEnvelope::from_envelope(&envelope);
        wire.entries_root = encode_hex(&[0; 32]);
        assert!(matches!(
            wire.into_envelope(),
            Err(RetrieverError::EntriesRootMismatch)
        ));
    }

    #[test]
    fn failed_temp_creation_preserves_previous_complete_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("retriever.json");
        let mut retriever = KuRetriever::default();
        retriever.upsert_source(ObjectReference::new(1, [1; 32]), "old".into());
        retriever.save_atomic(&path, [1; 32]).unwrap();
        std::fs::write(path.with_extension("tmp"), b"occupied").unwrap();
        retriever.upsert_source(ObjectReference::new(1, [2; 32]), "new".into());
        assert!(retriever.save_atomic(&path, [2; 32]).is_err());
        assert_eq!(
            KuRetriever::load_envelope(&path).unwrap().source_root,
            [1; 32]
        );
    }

    #[test]
    fn atomic_update_and_delete_replace_the_previous_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("retriever.json");
        let first = ObjectReference::new(1, [1; 32]);
        let second = ObjectReference::new(1, [2; 32]);
        let mut retriever = KuRetriever::default();
        retriever.upsert_source(first.clone(), "old".into());
        retriever.save_atomic(&path, [1; 32]).unwrap();
        retriever.upsert_source(first.clone(), "updated".into());
        retriever.upsert_source(second.clone(), "additional".into());
        assert!(retriever.remove_source(&first));
        retriever.save_atomic(&path, [2; 32]).unwrap();
        let envelope = KuRetriever::load_for_source_root(&path, [2; 32]).unwrap();
        assert_eq!(
            envelope
                .entries
                .into_iter()
                .map(|entry| entry.subject)
                .collect::<Vec<_>>(),
            vec![second]
        );
    }
}
