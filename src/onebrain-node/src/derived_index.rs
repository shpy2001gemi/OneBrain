//! Rebuildable Base graph/search projections bound to canonical vNext roots.

use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use ku_core::foundation::schema_registry::{EVENT_TYPES_V1, OBJECT_KINDS_V1};
use ku_core::foundation::{
    authority_event_descriptor, decode_feed_inception, decode_knowledge_event,
    decode_knowledge_object, dr_m5_failpoint, event_author_feed, AcceptedRecordEntry,
    AtomicVerifiedBackend, CanonicalValue, EventType, KnownObjectKind, ObjectKind, ObjectSemantics,
    RedbVerifiedBackend, ReservedDomain, ResourceProfile, StoredRecordKind,
};
use serde::{Deserialize, Serialize};

pub const VNEXT_DERIVED_INDEX_PROFILE: &str = "onebrain/base-derived-index/1";
const MAPPING_REDUCER_ID: &str = "base-v1-derived-projection-reducer/1";
const POINTER_FILE: &str = "current.json";

pub trait AcceptedRecordScan: Send + Sync {
    fn accepted_records(&self) -> Result<Vec<AcceptedRecordEntry>, DerivedIndexError>;
}

pub struct RedbAcceptedRecordScan {
    path: PathBuf,
}

impl RedbAcceptedRecordScan {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl AcceptedRecordScan for RedbAcceptedRecordScan {
    fn accepted_records(&self) -> Result<Vec<AcceptedRecordEntry>, DerivedIndexError> {
        let backend = RedbVerifiedBackend::open(&self.path).map_err(DerivedIndexError::Source)?;
        let mut records = Vec::new();
        for kind in [
            StoredRecordKind::Object,
            StoredRecordKind::Event,
            StoredRecordKind::FeedInception,
            StoredRecordKind::AuthorityEvent,
        ] {
            records.extend(
                backend
                    .accepted_record_entries(kind)
                    .map_err(DerivedIndexError::Source)?,
            );
        }
        Ok(records)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DerivedIndexOpenState {
    Ready,
    Rebuilt,
    Degraded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VNextIndexParityReport {
    pub source_root: [u8; 32],
    pub secondary_root: [u8; 32],
    pub graph_root: [u8; 32],
    pub accepted_record_count: u64,
    pub mismatch_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct CoverageRow {
    record_kind: u8,
    cid: [u8; 32],
    mapping_id: Option<String>,
    reducer_version: u64,
    graph_rows: u32,
    search_rows: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct DerivedRow {
    source_root: [u8; 32],
    record_kind: u8,
    canonical_record_reference: [u8; 32],
    mapping_id: String,
    reducer_version: u64,
    output_key: Vec<u8>,
    output_value: Vec<u8>,
    index_root: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct DerivedIndexDocument {
    profile: String,
    mapping_digest: [u8; 32],
    source_root: [u8; 32],
    coverage: Vec<CoverageRow>,
    graph_rows: Vec<DerivedRow>,
    search_rows: Vec<DerivedRow>,
    graph_root: [u8; 32],
    secondary_root: [u8; 32],
    projection_root: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GenerationPointer {
    profile: String,
    mapping_digest: [u8; 32],
    source_root: [u8; 32],
    projection_root: [u8; 32],
    relative_generation: String,
}

pub struct VNextDerivedIndexManager {
    root: PathBuf,
    reader_leases: Arc<AtomicUsize>,
}

impl VNextDerivedIndexManager {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, DerivedIndexError> {
        let root = root.into();
        std::fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            reader_leases: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub fn open_or_rebuild(
        &self,
        source: &dyn AcceptedRecordScan,
    ) -> (
        DerivedIndexOpenState,
        Result<VNextIndexParityReport, DerivedIndexError>,
    ) {
        match self.verify_parity(source) {
            Ok(report) => (DerivedIndexOpenState::Ready, Ok(report)),
            Err(_) => match self.rebuild(source) {
                Ok(report) => (DerivedIndexOpenState::Rebuilt, Ok(report)),
                Err(error) => (DerivedIndexOpenState::Degraded, Err(error)),
            },
        }
    }

    pub fn verify_parity(
        &self,
        source: &dyn AcceptedRecordScan,
    ) -> Result<VNextIndexParityReport, DerivedIndexError> {
        let expected = build_document(source)?;
        let pointer = self.read_pointer()?;
        if pointer.profile != VNEXT_DERIVED_INDEX_PROFILE
            || pointer.mapping_digest != expected.mapping_digest
            || pointer.source_root != expected.source_root
            || pointer.projection_root != expected.projection_root
        {
            return Err(DerivedIndexError::Parity);
        }
        let path = self
            .root
            .join(&pointer.relative_generation)
            .join("index.json");
        ensure_beneath(&self.root, &path)?;
        let bytes = std::fs::read(path)?;
        let actual: DerivedIndexDocument =
            serde_json::from_slice(&bytes).map_err(|_| DerivedIndexError::Corrupt)?;
        if actual != expected {
            return Err(DerivedIndexError::Parity);
        }
        Ok(report(&actual, 0))
    }

    pub fn rebuild(
        &self,
        source: &dyn AcceptedRecordScan,
    ) -> Result<VNextIndexParityReport, DerivedIndexError> {
        let document = build_document(source)?;
        let old = self.read_pointer().ok();
        let relative = generation_relative(document.mapping_digest, document.source_root);
        let generation = self.root.join(&relative);
        std::fs::create_dir_all(&generation)?;
        ensure_beneath(&self.root, &generation)?;
        write_atomic_json(&generation.join("index.json"), &document)?;
        let pointer = GenerationPointer {
            profile: VNEXT_DERIVED_INDEX_PROFILE.to_string(),
            mapping_digest: document.mapping_digest,
            source_root: document.source_root,
            projection_root: document.projection_root,
            relative_generation: relative.clone(),
        };
        dr_m5_failpoint::hit("TX-IDX-001", "before_begin_write");
        dr_m5_failpoint::hit("TX-IDX-001", "after_begin_write_before_mutation");
        write_atomic_json(&self.root.join(POINTER_FILE), &pointer)?;
        dr_m5_failpoint::hit("TX-IDX-001", "after_mutation_before_commit");
        dr_m5_failpoint::hit("TX-IDX-001", "after_commit_before_next_side_effect");
        if self.reader_leases.load(Ordering::Acquire) == 0 {
            if let Some(old) = old.filter(|old| old.relative_generation != relative) {
                let old_path = self.root.join(old.relative_generation);
                ensure_beneath(&self.root, &old_path)?;
                if old_path.exists() {
                    std::fs::remove_dir_all(old_path)?;
                }
            }
        }
        dr_m5_failpoint::hit("TX-IDX-001", "after_next_side_effect_before_ack");
        Ok(report(&document, 0))
    }

    pub fn acquire_reader(&self) -> DerivedIndexReaderLease {
        self.reader_leases.fetch_add(1, Ordering::AcqRel);
        DerivedIndexReaderLease {
            count: self.reader_leases.clone(),
        }
    }

    pub fn current_generation_path(&self) -> Result<PathBuf, DerivedIndexError> {
        let pointer = self.read_pointer()?;
        let path = self.root.join(pointer.relative_generation);
        ensure_beneath(&self.root, &path)?;
        Ok(path)
    }

    fn read_pointer(&self) -> Result<GenerationPointer, DerivedIndexError> {
        let bytes = std::fs::read(self.root.join(POINTER_FILE))?;
        serde_json::from_slice(&bytes).map_err(|_| DerivedIndexError::Corrupt)
    }
}

pub struct DerivedIndexReaderLease {
    count: Arc<AtomicUsize>,
}

impl Drop for DerivedIndexReaderLease {
    fn drop(&mut self) {
        self.count.fetch_sub(1, Ordering::AcqRel);
    }
}

fn build_document(
    source: &dyn AcceptedRecordScan,
) -> Result<DerivedIndexDocument, DerivedIndexError> {
    let mut entries = source.accepted_records()?;
    entries.sort_by_key(|entry| (entry.record_kind as u8, entry.claimed_cid));
    let mut identities = BTreeSet::new();
    if entries
        .iter()
        .any(|entry| !identities.insert((entry.record_kind as u8, entry.claimed_cid)))
    {
        return Err(DerivedIndexError::DuplicateCanonicalIdentity);
    }
    validate_entries(&entries)?;
    let source_root = record_root(&entries);
    let mapping_digest = mapping_digest();
    let known_objects = OBJECT_KINDS_V1
        .iter()
        .map(|entry| KnownObjectKind::new(ObjectKind(entry.id), 1))
        .collect::<Vec<_>>();
    let known_events = EVENT_TYPES_V1
        .iter()
        .map(|entry| EventType(entry.id))
        .collect::<Vec<_>>();
    let feeds = entries
        .iter()
        .filter(|entry| entry.record_kind == StoredRecordKind::FeedInception)
        .map(|entry| {
            decode_feed_inception(&entry.canonical_bytes).map_err(|_| DerivedIndexError::Corrupt)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut coverage = Vec::new();
    let mut graph_rows = Vec::new();
    let mut search_rows = Vec::new();
    for entry in &entries {
        let (mapping, graph_values, search_values) = match entry.record_kind {
            StoredRecordKind::Object => {
                let object = decode_knowledge_object(
                    &entry.canonical_bytes,
                    ResourceProfile::ObjectV1,
                    &known_objects,
                    &[],
                )
                .map_err(|_| DerivedIndexError::Corrupt)?;
                match object.semantics() {
                    ObjectSemantics::Known(envelope) => {
                        let mapping = object_mapping(envelope.kind.0)?;
                        let graph = envelope
                            .references
                            .iter()
                            .map(|reference| {
                                reference_bytes(reference.reference_kind, reference.cid)
                            })
                            .collect::<Vec<_>>();
                        let mut text = Vec::new();
                        collect_text(&envelope.payload, &mut text);
                        let search = if envelope.kind.0 == 7 {
                            Vec::new()
                        } else {
                            text.into_iter().map(String::into_bytes).collect()
                        };
                        (Some(mapping), graph, search)
                    }
                    ObjectSemantics::Opaque { .. } => (None, Vec::new(), Vec::new()),
                }
            }
            StoredRecordKind::Event => {
                let feed_id = event_author_feed(&entry.canonical_bytes)
                    .map_err(|_| DerivedIndexError::Corrupt)?;
                let event = feeds
                    .iter()
                    .find_map(|feed| {
                        decode_knowledge_event(&entry.canonical_bytes, feed, &known_events)
                            .ok()
                            .filter(|event| event.signed.event.author_feed == feed_id)
                    })
                    .ok_or(DerivedIndexError::Corrupt)?;
                let mapping = event_mapping(event.signed.event.event_type.0)?;
                let mut graph = event
                    .signed
                    .event
                    .payload_refs
                    .iter()
                    .map(|reference| reference_bytes(reference.reference_kind, reference.cid))
                    .collect::<Vec<_>>();
                graph.extend(
                    event
                        .signed
                        .event
                        .causal_parents
                        .iter()
                        .map(|parent| parent.as_bytes().to_vec()),
                );
                (Some(mapping), graph, Vec::new())
            }
            StoredRecordKind::FeedInception | StoredRecordKind::AuthorityEvent => {
                (None, Vec::new(), Vec::new())
            }
        };
        let mapping_id = mapping.clone();
        if let Some(mapping) = mapping {
            for (ordinal, value) in graph_values.iter().enumerate() {
                graph_rows.push(row(source_root, entry, &mapping, ordinal, value.clone()));
            }
            for (ordinal, value) in search_values.iter().enumerate() {
                search_rows.push(row(source_root, entry, &mapping, ordinal, value.clone()));
            }
        }
        coverage.push(CoverageRow {
            record_kind: entry.record_kind as u8,
            cid: entry.claimed_cid,
            mapping_id,
            reducer_version: 1,
            graph_rows: graph_values.len() as u32,
            search_rows: search_values.len() as u32,
        });
    }
    graph_rows.sort_by(row_order);
    search_rows.sort_by(row_order);
    let graph_root = rows_root(b"onebrain:base-v1:graph-index-root:1\0", &graph_rows);
    let secondary_root = rows_root(b"onebrain:base-v1:search-index-root:1\0", &search_rows);
    for row in &mut graph_rows {
        row.index_root = graph_root;
    }
    for row in &mut search_rows {
        row.index_root = secondary_root;
    }
    let projection_root = projection_root(
        mapping_digest,
        source_root,
        graph_root,
        secondary_root,
        &coverage,
    );
    Ok(DerivedIndexDocument {
        profile: VNEXT_DERIVED_INDEX_PROFILE.to_string(),
        mapping_digest,
        source_root,
        coverage,
        graph_rows,
        search_rows,
        graph_root,
        secondary_root,
        projection_root,
    })
}

fn validate_entries(entries: &[AcceptedRecordEntry]) -> Result<(), DerivedIndexError> {
    for entry in entries {
        let computed = match entry.record_kind {
            StoredRecordKind::Object => ReservedDomain::Object.digest(&entry.canonical_bytes),
            StoredRecordKind::Event => ReservedDomain::Event.digest(&entry.canonical_bytes),
            StoredRecordKind::FeedInception => {
                ReservedDomain::FeedInception.digest(&entry.canonical_bytes)
            }
            StoredRecordKind::AuthorityEvent => {
                authority_event_descriptor(&entry.canonical_bytes)
                    .map_err(|_| DerivedIndexError::Corrupt)?;
                ReservedDomain::AuthorityEvent.digest(&entry.canonical_bytes)
            }
        };
        if computed != entry.claimed_cid {
            return Err(DerivedIndexError::CanonicalCidMismatch);
        }
    }
    Ok(())
}

fn object_mapping(kind: u64) -> Result<String, DerivedIndexError> {
    let name = OBJECT_KINDS_V1
        .iter()
        .find(|entry| entry.id == kind)
        .map(|entry| entry.name)
        .ok_or(DerivedIndexError::UnknownMapping)?;
    Ok(format!("base-v1/object/{kind}-{name}/1"))
}

fn event_mapping(kind: u64) -> Result<String, DerivedIndexError> {
    let name = EVENT_TYPES_V1
        .iter()
        .find(|entry| entry.id == kind)
        .map(|entry| entry.name)
        .ok_or(DerivedIndexError::UnknownMapping)?;
    Ok(format!("base-v1/event/{kind}-{name}/1"))
}

fn mapping_digest() -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:base-v1:derived-mapping-profile:1\0");
    hasher.update(MAPPING_REDUCER_ID.as_bytes());
    for entry in OBJECT_KINDS_V1 {
        hasher.update(&entry.id.to_be_bytes());
        hasher.update(
            object_mapping(entry.id)
                .expect("registry entry maps")
                .as_bytes(),
        );
    }
    for entry in EVENT_TYPES_V1 {
        hasher.update(&entry.id.to_be_bytes());
        hasher.update(
            event_mapping(entry.id)
                .expect("registry entry maps")
                .as_bytes(),
        );
    }
    *hasher.finalize().as_bytes()
}

fn record_root(entries: &[AcceptedRecordEntry]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:base-v1:accepted-record-source-root:1\0");
    for entry in entries {
        hasher.update(&[entry.record_kind as u8]);
        hasher.update(&entry.claimed_cid);
        hasher.update(&(entry.canonical_bytes.len() as u64).to_be_bytes());
        hasher.update(&entry.canonical_bytes);
    }
    *hasher.finalize().as_bytes()
}

fn row(
    source_root: [u8; 32],
    entry: &AcceptedRecordEntry,
    mapping_id: &str,
    ordinal: usize,
    output_value: Vec<u8>,
) -> DerivedRow {
    let mut output_key = Vec::with_capacity(41);
    output_key.push(entry.record_kind as u8);
    output_key.extend_from_slice(&entry.claimed_cid);
    output_key.extend_from_slice(&(ordinal as u64).to_be_bytes());
    DerivedRow {
        source_root,
        record_kind: entry.record_kind as u8,
        canonical_record_reference: entry.claimed_cid,
        mapping_id: mapping_id.to_string(),
        reducer_version: 1,
        output_key,
        output_value,
        index_root: [0; 32],
    }
}

fn row_order(left: &DerivedRow, right: &DerivedRow) -> std::cmp::Ordering {
    (
        left.record_kind,
        left.canonical_record_reference,
        &left.mapping_id,
        &left.output_key,
        &left.output_value,
    )
        .cmp(&(
            right.record_kind,
            right.canonical_record_reference,
            &right.mapping_id,
            &right.output_key,
            &right.output_value,
        ))
}

fn rows_root(domain: &[u8], rows: &[DerivedRow]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    for row in rows {
        hasher.update(&[row.record_kind]);
        hasher.update(&row.canonical_record_reference);
        hasher.update(row.mapping_id.as_bytes());
        hasher.update(&row.reducer_version.to_be_bytes());
        hasher.update(&(row.output_key.len() as u64).to_be_bytes());
        hasher.update(&row.output_key);
        hasher.update(&(row.output_value.len() as u64).to_be_bytes());
        hasher.update(&row.output_value);
    }
    *hasher.finalize().as_bytes()
}

fn projection_root(
    mapping_digest: [u8; 32],
    source_root: [u8; 32],
    graph_root: [u8; 32],
    secondary_root: [u8; 32],
    coverage: &[CoverageRow],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:base-v1:derived-projection-root:1\0");
    hasher.update(&mapping_digest);
    hasher.update(&source_root);
    hasher.update(&graph_root);
    hasher.update(&secondary_root);
    hasher.update(&serde_json::to_vec(coverage).expect("coverage serializes"));
    *hasher.finalize().as_bytes()
}

fn collect_text(value: &CanonicalValue, output: &mut Vec<String>) {
    match value {
        CanonicalValue::Text(text) => output.push(text.clone()),
        CanonicalValue::Array(values) => {
            for value in values {
                collect_text(value, output);
            }
        }
        CanonicalValue::Map(fields) => {
            for (_, value) in fields {
                collect_text(value, output);
            }
        }
        _ => {}
    }
}

fn reference_bytes(kind: u64, cid: [u8; 32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(40);
    bytes.extend_from_slice(&kind.to_be_bytes());
    bytes.extend_from_slice(&cid);
    bytes
}

fn generation_relative(mapping: [u8; 32], source: [u8; 32]) -> String {
    format!(
        "derived/{}/{}/{}",
        VNEXT_DERIVED_INDEX_PROFILE.replace('/', "_"),
        hex(&mapping),
        hex(&source)
    )
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_atomic_json(path: &Path, value: &impl Serialize) -> Result<(), DerivedIndexError> {
    let parent = path.parent().ok_or(DerivedIndexError::PathEscape)?;
    std::fs::create_dir_all(parent)?;
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    let bytes = serde_json::to_vec(value).map_err(|_| DerivedIndexError::Corrupt)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    output.write_all(&bytes)?;
    output.sync_all()?;
    drop(output);
    std::fs::rename(&temp, path)?;
    sync_directory(parent)?;
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), DerivedIndexError> {
    std::fs::File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), DerivedIndexError> {
    Ok(())
}

fn ensure_beneath(root: &Path, target: &Path) -> Result<(), DerivedIndexError> {
    let root = root.canonicalize()?;
    let candidate = if target.exists() {
        target.canonicalize()?
    } else {
        target
            .parent()
            .ok_or(DerivedIndexError::PathEscape)?
            .canonicalize()?
            .join(target.file_name().ok_or(DerivedIndexError::PathEscape)?)
    };
    if candidate.starts_with(root) {
        Ok(())
    } else {
        Err(DerivedIndexError::PathEscape)
    }
}

fn report(document: &DerivedIndexDocument, mismatch_count: u64) -> VNextIndexParityReport {
    VNextIndexParityReport {
        source_root: document.source_root,
        secondary_root: document.secondary_root,
        graph_root: document.graph_root,
        accepted_record_count: document.coverage.len() as u64,
        mismatch_count,
    }
}

#[derive(Debug)]
pub enum DerivedIndexError {
    Io(std::io::Error),
    Source(String),
    Corrupt,
    Parity,
    CanonicalCidMismatch,
    DuplicateCanonicalIdentity,
    UnknownMapping,
    PathEscape,
}

impl std::fmt::Display for DerivedIndexError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl From<std::io::Error> for DerivedIndexError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
