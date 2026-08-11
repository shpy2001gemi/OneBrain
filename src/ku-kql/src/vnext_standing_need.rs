//! Durable local StandingNeeds and rebuildable minimal Receptor/Mapping views.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Mutex;

use ku_core::foundation::resolution::RESOLUTION_REDUCER_VERSION;
use ku_core::foundation::{
    decode_canonical, encode_canonical, CanonicalError, CanonicalValue, DisclosureClass,
    MappingKernelCid, ObjectCid, ObjectReference, ResolutionState, ResolutionTarget,
    ResolutionView, ResourceProfile, SelectorCid,
};

pub const STANDING_NEED_PROFILE_MAJOR: u64 = 1;
pub const STANDING_NEED_PROFILE_MINOR: u64 = 0;
pub const MINIMAL_VIEW_REDUCER_VERSION: u64 = 1;
pub const MAX_STANDING_NEEDS: usize = 1_000_000;

type StandingNeedRecord = (u64, Vec<u8>);
type ResolutionStateEntry = (ResolutionTarget, ResolutionState);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StandingNeedId([u8; 32]);

impl StandingNeedId {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandingNeedState {
    Active,
    Paused,
    Retired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandingNeedOrigin {
    Native,
    LegacyWatchImport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandingNeed {
    pub receptor_definition: ObjectReference,
    pub query_definition: ObjectCid,
    pub selector: SelectorCid,
    pub watch_policy: ObjectReference,
    pub generation: u64,
    pub state: StandingNeedState,
    pub observed_frontier: [u8; 32],
    pub origin: StandingNeedOrigin,
    pub privacy: DisclosureClass,
}

impl StandingNeed {
    pub fn new_local(
        receptor_definition: ObjectReference,
        query_definition: ObjectCid,
        selector: SelectorCid,
        watch_policy: ObjectReference,
        observed_frontier: [u8; 32],
    ) -> Self {
        Self {
            receptor_definition,
            query_definition,
            selector,
            watch_policy,
            generation: 0,
            state: StandingNeedState::Active,
            observed_frontier,
            origin: StandingNeedOrigin::Native,
            privacy: DisclosureClass::LocalOnly,
        }
    }

    pub fn import_legacy_watch(
        receptor_definition: ObjectReference,
        query_definition: ObjectCid,
        selector: SelectorCid,
        watch_policy: ObjectReference,
        observed_frontier: [u8; 32],
    ) -> Self {
        let mut need = Self::new_local(
            receptor_definition,
            query_definition,
            selector,
            watch_policy,
            observed_frontier,
        );
        need.origin = StandingNeedOrigin::LegacyWatchImport;
        need
    }

    pub fn validate(&self) -> Result<(), StandingNeedError> {
        if self.privacy != DisclosureClass::LocalOnly {
            return Err(StandingNeedError::MustRemainLocalOnly);
        }
        if self.observed_frontier == [0; 32] {
            return Err(StandingNeedError::InvalidFrontier);
        }
        Ok(())
    }

    pub fn id(&self) -> Result<StandingNeedId, StandingNeedError> {
        self.validate()?;
        let identity = CanonicalValue::Map(vec![
            (0, reference_value(&self.receptor_definition)),
            (
                1,
                CanonicalValue::Bytes(self.query_definition.as_bytes().to_vec()),
            ),
            (2, CanonicalValue::Bytes(self.selector.as_bytes().to_vec())),
            (3, reference_value(&self.watch_policy)),
            (4, CanonicalValue::Unsigned(origin_code(self.origin))),
        ]);
        let bytes = encode_canonical(&identity, ResourceProfile::ObjectV1)?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:local-standing-need:1\0");
        hasher.update(&bytes);
        Ok(StandingNeedId(*hasher.finalize().as_bytes()))
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, StandingNeedError> {
        self.validate()?;
        let value = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(STANDING_NEED_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(STANDING_NEED_PROFILE_MINOR)),
            (2, reference_value(&self.receptor_definition)),
            (
                3,
                CanonicalValue::Bytes(self.query_definition.as_bytes().to_vec()),
            ),
            (4, CanonicalValue::Bytes(self.selector.as_bytes().to_vec())),
            (5, reference_value(&self.watch_policy)),
            (6, CanonicalValue::Unsigned(self.generation)),
            (7, CanonicalValue::Unsigned(state_code(self.state))),
            (8, CanonicalValue::Bytes(self.observed_frontier.to_vec())),
            (9, CanonicalValue::Unsigned(origin_code(self.origin))),
            (10, CanonicalValue::Unsigned(self.privacy as u64)),
        ]);
        encode_canonical(&value, ResourceProfile::ObjectV1).map_err(Into::into)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, StandingNeedError> {
        let value = decode_canonical(bytes, ResourceProfile::ObjectV1)?;
        let map = as_map(&value, "standing_need")?;
        if unsigned(map, 0, "major")? != STANDING_NEED_PROFILE_MAJOR {
            return Err(StandingNeedError::UnsupportedVersion);
        }
        let privacy = match unsigned(map, 10, "privacy")? {
            3 => DisclosureClass::LocalOnly,
            _ => return Err(StandingNeedError::MustRemainLocalOnly),
        };
        let need = Self {
            receptor_definition: parse_reference(required(map, 2, "receptor")?)?,
            query_definition: ObjectCid::from_bytes(bytes32(map, 3, "query_definition")?),
            selector: SelectorCid::from_bytes(bytes32(map, 4, "selector")?),
            watch_policy: parse_reference(required(map, 5, "watch_policy")?)?,
            generation: unsigned(map, 6, "generation")?,
            state: match unsigned(map, 7, "state")? {
                0 => StandingNeedState::Active,
                1 => StandingNeedState::Paused,
                2 => StandingNeedState::Retired,
                _ => return Err(StandingNeedError::InvalidField("state")),
            },
            observed_frontier: bytes32(map, 8, "frontier")?,
            origin: match unsigned(map, 9, "origin")? {
                0 => StandingNeedOrigin::Native,
                1 => StandingNeedOrigin::LegacyWatchImport,
                _ => return Err(StandingNeedError::InvalidField("origin")),
            },
            privacy,
        };
        need.validate()?;
        if need.canonical_bytes()? != bytes {
            return Err(StandingNeedError::NonCanonicalRecord);
        }
        Ok(need)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandingNeedWriteOutcome {
    Stored,
    Updated,
    ExactReplay,
    StaleGeneration,
    GenerationConflict,
}

pub trait StandingNeedBackend: Send + Sync {
    fn put_generation(
        &self,
        id: StandingNeedId,
        generation: u64,
        bytes: &[u8],
    ) -> Result<StandingNeedWriteOutcome, String>;

    fn get(&self, id: StandingNeedId) -> Result<Option<Vec<u8>>, String>;

    fn list(&self) -> Result<Vec<Vec<u8>>, String>;
}

#[derive(Default)]
pub struct InMemoryStandingNeedBackend {
    records: Mutex<HashMap<[u8; 32], StandingNeedRecord>>,
}

impl StandingNeedBackend for InMemoryStandingNeedBackend {
    fn put_generation(
        &self,
        id: StandingNeedId,
        generation: u64,
        bytes: &[u8],
    ) -> Result<StandingNeedWriteOutcome, String> {
        let mut records = self
            .records
            .lock()
            .map_err(|_| "STANDING_NEED_LOCK_POISONED".to_string())?;
        let outcome = match records.get(id.as_bytes()) {
            Some((existing_generation, existing)) if *existing_generation > generation => {
                StandingNeedWriteOutcome::StaleGeneration
            }
            Some((existing_generation, existing)) if *existing_generation == generation => {
                if existing == bytes {
                    StandingNeedWriteOutcome::ExactReplay
                } else {
                    StandingNeedWriteOutcome::GenerationConflict
                }
            }
            Some(_) => {
                records.insert(*id.as_bytes(), (generation, bytes.to_vec()));
                StandingNeedWriteOutcome::Updated
            }
            None => {
                records.insert(*id.as_bytes(), (generation, bytes.to_vec()));
                StandingNeedWriteOutcome::Stored
            }
        };
        Ok(outcome)
    }

    fn get(&self, id: StandingNeedId) -> Result<Option<Vec<u8>>, String> {
        Ok(self
            .records
            .lock()
            .map_err(|_| "STANDING_NEED_LOCK_POISONED".to_string())?
            .get(id.as_bytes())
            .map(|(_, bytes)| bytes.clone()))
    }

    fn list(&self) -> Result<Vec<Vec<u8>>, String> {
        let records = self
            .records
            .lock()
            .map_err(|_| "STANDING_NEED_LOCK_POISONED".to_string())?;
        let mut records = records
            .iter()
            .map(|(id, (_, bytes))| (*id, bytes.clone()))
            .collect::<Vec<_>>();
        records.sort_by_key(|(id, _)| *id);
        Ok(records.into_iter().map(|(_, bytes)| bytes).collect())
    }
}

pub struct StandingNeedStore<B> {
    backend: B,
}

impl<B: StandingNeedBackend> StandingNeedStore<B> {
    pub const fn new(backend: B) -> Self {
        Self { backend }
    }

    pub fn put(
        &self,
        need: &StandingNeed,
    ) -> Result<(StandingNeedId, StandingNeedWriteOutcome), StandingNeedError> {
        let id = need.id()?;
        let bytes = need.canonical_bytes()?;
        let outcome = self
            .backend
            .put_generation(id, need.generation, &bytes)
            .map_err(StandingNeedError::Backend)?;
        Ok((id, outcome))
    }

    pub fn get(&self, id: StandingNeedId) -> Result<Option<StandingNeed>, StandingNeedError> {
        self.backend
            .get(id)
            .map_err(StandingNeedError::Backend)?
            .map(|bytes| StandingNeed::decode(&bytes))
            .transpose()
    }

    pub fn load_all(&self) -> Result<Vec<StandingNeed>, StandingNeedError> {
        let records = self.backend.list().map_err(StandingNeedError::Backend)?;
        if records.len() > MAX_STANDING_NEEDS {
            return Err(StandingNeedError::Limit);
        }
        records
            .into_iter()
            .map(|bytes| StandingNeed::decode(&bytes))
            .collect()
    }
}

#[cfg(feature = "storage")]
mod persistent {
    use std::path::Path;

    use redb::{Database, ReadableTable, TableDefinition};

    use super::*;

    const NEEDS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("vnext_local_standing_needs");

    pub struct RedbStandingNeedBackend {
        database: Database,
    }

    impl RedbStandingNeedBackend {
        pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
            Database::create(path)
                .map(|database| Self { database })
                .map_err(|error| error.to_string())
        }
    }

    impl StandingNeedBackend for RedbStandingNeedBackend {
        fn put_generation(
            &self,
            id: StandingNeedId,
            generation: u64,
            bytes: &[u8],
        ) -> Result<StandingNeedWriteOutcome, String> {
            let write = self
                .database
                .begin_write()
                .map_err(|error| error.to_string())?;
            let outcome;
            {
                let mut table = write.open_table(NEEDS).map_err(|error| error.to_string())?;
                let existing = table
                    .get(id.as_bytes().as_slice())
                    .map_err(|error| error.to_string())?
                    .map(|value| value.value().to_vec());
                outcome = match existing {
                    Some(existing) => {
                        let current = StandingNeed::decode(&existing)
                            .map_err(|error| format!("{error:?}"))?;
                        if current.generation > generation {
                            StandingNeedWriteOutcome::StaleGeneration
                        } else if current.generation == generation {
                            if existing == bytes {
                                StandingNeedWriteOutcome::ExactReplay
                            } else {
                                StandingNeedWriteOutcome::GenerationConflict
                            }
                        } else {
                            table
                                .insert(id.as_bytes().as_slice(), bytes)
                                .map_err(|error| error.to_string())?;
                            StandingNeedWriteOutcome::Updated
                        }
                    }
                    None => {
                        table
                            .insert(id.as_bytes().as_slice(), bytes)
                            .map_err(|error| error.to_string())?;
                        StandingNeedWriteOutcome::Stored
                    }
                };
            }
            write.commit().map_err(|error| error.to_string())?;
            Ok(outcome)
        }

        fn get(&self, id: StandingNeedId) -> Result<Option<Vec<u8>>, String> {
            let read = self
                .database
                .begin_read()
                .map_err(|error| error.to_string())?;
            let table = read.open_table(NEEDS).map_err(|error| error.to_string())?;
            table
                .get(id.as_bytes().as_slice())
                .map_err(|error| error.to_string())
                .map(|value| value.map(|value| value.value().to_vec()))
        }

        fn list(&self) -> Result<Vec<Vec<u8>>, String> {
            let read = self
                .database
                .begin_read()
                .map_err(|error| error.to_string())?;
            let table = read.open_table(NEEDS).map_err(|error| error.to_string())?;
            table
                .iter()
                .map_err(|error| error.to_string())?
                .map(|entry| {
                    entry
                        .map(|(_, value)| value.value().to_vec())
                        .map_err(|error| error.to_string())
                })
                .collect()
        }
    }
}

#[cfg(feature = "storage")]
pub use persistent::RedbStandingNeedBackend;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceptorResolutionProjection {
    pub receptor_definition: ObjectReference,
    pub view: ResolutionView,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappingViewRecord {
    pub kernel: MappingKernelCid,
    pub envelope: ObjectCid,
    pub disclosure: DisclosureClass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MinimalViewSnapshot {
    pub source_root: [u8; 32],
    pub projection_root: [u8; 32],
    pub reducer_version: u64,
    pub resolution_reducer_version: u64,
}

#[derive(Clone, Debug, Default)]
pub struct MinimalKnowledgeViews {
    receptor_needs: BTreeMap<(u64, [u8; 32]), BTreeSet<[u8; 32]>>,
    receptor_resolutions: BTreeMap<(u64, [u8; 32]), Vec<ResolutionStateEntry>>,
    mappings: BTreeMap<[u8; 32], MappingViewRecord>,
    mapping_adoptions: BTreeMap<[u8; 32], BTreeSet<ResolutionTargetKey>>,
    snapshot: Option<MinimalViewSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ResolutionTargetKey {
    lineage: [u8; 32],
    revision: [u8; 32],
    placement: [u8; 32],
}

impl From<ResolutionTarget> for ResolutionTargetKey {
    fn from(target: ResolutionTarget) -> Self {
        Self {
            lineage: *target.assembly_lineage.as_bytes(),
            revision: target.assembly_revision.into_bytes(),
            placement: *target.placement.as_bytes(),
        }
    }
}

impl MinimalKnowledgeViews {
    pub fn rebuild(
        needs: &[StandingNeed],
        resolutions: &[ReceptorResolutionProjection],
        mappings: &[MappingViewRecord],
    ) -> Result<Self, StandingNeedError> {
        let mut view = Self::default();
        let mut source_keys = BTreeSet::new();
        for need in needs {
            let id = need.id()?;
            source_keys.insert((0u8, *id.as_bytes()));
            view.receptor_needs
                .entry(reference_key(&need.receptor_definition))
                .or_default()
                .insert(*id.as_bytes());
        }
        for resolution in resolutions {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"onebrain:vnext:resolution-view-source:1\0");
            hasher.update(resolution.view.target.assembly_lineage.as_bytes());
            hasher.update(resolution.view.target.assembly_revision.as_bytes());
            hasher.update(resolution.view.target.placement.as_bytes());
            hasher.update(&[resolution_state_code(resolution.view.state)]);
            let source_id = *hasher.finalize().as_bytes();
            source_keys.insert((1, source_id));
            view.receptor_resolutions
                .entry(reference_key(&resolution.receptor_definition))
                .or_default()
                .push((resolution.view.target, resolution.view.state));
            for branch in &resolution.view.branches {
                for mapping in &branch.active_bindings {
                    view.mapping_adoptions
                        .entry(mapping.into_bytes())
                        .or_default()
                        .insert(resolution.view.target.into());
                }
            }
        }
        for mapping in mappings {
            source_keys.insert((2, mapping.envelope.into_bytes()));
            view.mappings
                .insert(mapping.kernel.into_bytes(), mapping.clone());
        }
        for values in view.receptor_resolutions.values_mut() {
            values.sort_by_key(|(target, state)| {
                (
                    *target.assembly_lineage.as_bytes(),
                    target.assembly_revision.into_bytes(),
                    *target.placement.as_bytes(),
                    resolution_state_code(*state),
                )
            });
        }
        let source_root = hash_source_keys(&source_keys);
        let projection_root = view.projection_root();
        view.snapshot = Some(MinimalViewSnapshot {
            source_root,
            projection_root,
            reducer_version: MINIMAL_VIEW_REDUCER_VERSION,
            resolution_reducer_version: RESOLUTION_REDUCER_VERSION,
        });
        Ok(view)
    }

    pub fn snapshot(&self) -> Option<MinimalViewSnapshot> {
        self.snapshot
    }

    pub fn standing_needs_for(&self, receptor: &ObjectReference) -> Vec<StandingNeedId> {
        self.receptor_needs
            .get(&reference_key(receptor))
            .into_iter()
            .flatten()
            .copied()
            .map(StandingNeedId::from_bytes)
            .collect()
    }

    pub fn resolutions_for(
        &self,
        receptor: &ObjectReference,
    ) -> Vec<(ResolutionTarget, ResolutionState)> {
        self.receptor_resolutions
            .get(&reference_key(receptor))
            .cloned()
            .unwrap_or_default()
    }

    pub fn mapping(&self, kernel: MappingKernelCid) -> Option<&MappingViewRecord> {
        self.mappings.get(kernel.as_bytes())
    }

    pub fn adopted_targets(&self, kernel: MappingKernelCid) -> usize {
        self.mapping_adoptions
            .get(kernel.as_bytes())
            .map_or(0, BTreeSet::len)
    }

    fn projection_root(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:minimal-knowledge-views:1\0");
        for (key, needs) in &self.receptor_needs {
            hash_reference_key(&mut hasher, key);
            for need in needs {
                hasher.update(need);
            }
        }
        for (key, resolutions) in &self.receptor_resolutions {
            hash_reference_key(&mut hasher, key);
            for (target, state) in resolutions {
                hasher.update(target.assembly_lineage.as_bytes());
                hasher.update(target.assembly_revision.as_bytes());
                hasher.update(target.placement.as_bytes());
                hasher.update(&[resolution_state_code(*state)]);
            }
        }
        for (kernel, mapping) in &self.mappings {
            hasher.update(kernel);
            hasher.update(mapping.envelope.as_bytes());
            hasher.update(&[mapping.disclosure as u8]);
        }
        for (kernel, targets) in &self.mapping_adoptions {
            hasher.update(kernel);
            for target in targets {
                hasher.update(&target.lineage);
                hasher.update(&target.revision);
                hasher.update(&target.placement);
            }
        }
        *hasher.finalize().as_bytes()
    }
}

fn reference_value(reference: &ObjectReference) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(reference.reference_kind)),
        (1, CanonicalValue::Bytes(reference.cid.to_vec())),
    ])
}

fn reference_key(reference: &ObjectReference) -> (u64, [u8; 32]) {
    (reference.reference_kind, reference.cid)
}

fn state_code(state: StandingNeedState) -> u64 {
    match state {
        StandingNeedState::Active => 0,
        StandingNeedState::Paused => 1,
        StandingNeedState::Retired => 2,
    }
}

fn origin_code(origin: StandingNeedOrigin) -> u64 {
    match origin {
        StandingNeedOrigin::Native => 0,
        StandingNeedOrigin::LegacyWatchImport => 1,
    }
}

fn resolution_state_code(state: ResolutionState) -> u8 {
    match state {
        ResolutionState::Open => 0,
        ResolutionState::PartiallySatisfied => 1,
        ResolutionState::SatisfiedRelative => 2,
        ResolutionState::Waived => 3,
        ResolutionState::Deferred => 4,
        ResolutionState::Concurrent => 5,
    }
}

fn hash_source_keys(keys: &BTreeSet<(u8, [u8; 32])>) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:minimal-view-sources:1\0");
    for (kind, cid) in keys {
        hasher.update(&[*kind]);
        hasher.update(cid);
    }
    *hasher.finalize().as_bytes()
}

fn hash_reference_key(hasher: &mut blake3::Hasher, key: &(u64, [u8; 32])) {
    hasher.update(&key.0.to_be_bytes());
    hasher.update(&key.1);
}

fn as_map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], StandingNeedError> {
    match value {
        CanonicalValue::Map(map) => Ok(map),
        _ => Err(StandingNeedError::InvalidField(field)),
    }
}

fn required<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, StandingNeedError> {
    map.iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(StandingNeedError::InvalidField(field))
}

fn unsigned(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, StandingNeedError> {
    match required(map, key, field)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(StandingNeedError::InvalidField(field)),
    }
}

fn bytes32(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], StandingNeedError> {
    match required(map, key, field)? {
        CanonicalValue::Bytes(bytes) if bytes.len() == 32 => {
            let mut output = [0; 32];
            output.copy_from_slice(bytes);
            Ok(output)
        }
        _ => Err(StandingNeedError::InvalidField(field)),
    }
}

fn parse_reference(value: &CanonicalValue) -> Result<ObjectReference, StandingNeedError> {
    let map = as_map(value, "reference")?;
    Ok(ObjectReference::new(
        unsigned(map, 0, "reference.kind")?,
        bytes32(map, 1, "reference.cid")?,
    ))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StandingNeedError {
    Canonical(CanonicalError),
    Backend(String),
    InvalidField(&'static str),
    UnsupportedVersion,
    NonCanonicalRecord,
    MustRemainLocalOnly,
    InvalidFrontier,
    Limit,
}

impl From<CanonicalError> for StandingNeedError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

#[cfg(test)]
mod tests {
    use ku_core::foundation::{AssemblyLineageId, PlacementId, ResolutionBranch, ResolutionTarget};

    use super::*;

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn need() -> StandingNeed {
        StandingNeed::new_local(
            reference(1),
            ObjectCid::from_bytes([2; 32]),
            SelectorCid::from_bytes([3; 32]),
            reference(4),
            [5; 32],
        )
    }

    fn resolution(mapping: MappingKernelCid) -> ReceptorResolutionProjection {
        let target = ResolutionTarget {
            assembly_lineage: AssemblyLineageId::from_bytes([10; 32]),
            assembly_revision: ObjectCid::from_bytes([11; 32]),
            placement: PlacementId::from_bytes([12; 32]),
        };
        ReceptorResolutionProjection {
            receptor_definition: reference(1),
            view: ResolutionView {
                target,
                state: ResolutionState::SatisfiedRelative,
                resolution_policy: reference(13),
                assessed_frontier: [14; 32],
                reducer_version: RESOLUTION_REDUCER_VERSION,
                branches: vec![ResolutionBranch {
                    tip_event: ku_core::foundation::EventCid::from_bytes([15; 32]),
                    state: ResolutionState::SatisfiedRelative,
                    active_bindings: vec![mapping],
                }],
                unresolved_events: Vec::new(),
            },
        }
    }

    #[test]
    fn standalone_need_round_trips_without_assembly_identity() {
        let need = need();
        let bytes = need.canonical_bytes().unwrap();
        assert_eq!(StandingNeed::decode(&bytes).unwrap(), need);
        assert!(!bytes.windows(32).any(|window| window == [99; 32]));
        let legacy = StandingNeed::import_legacy_watch(
            reference(1),
            ObjectCid::from_bytes([2; 32]),
            SelectorCid::from_bytes([3; 32]),
            reference(4),
            [5; 32],
        );
        assert_eq!(legacy.privacy, DisclosureClass::LocalOnly);
        assert_eq!(legacy.origin, StandingNeedOrigin::LegacyWatchImport);
    }

    #[test]
    fn store_survives_reload_and_rejects_stale_or_conflicting_generation() {
        let store = StandingNeedStore::new(InMemoryStandingNeedBackend::default());
        let mut need = need();
        let (id, outcome) = store.put(&need).unwrap();
        assert_eq!(outcome, StandingNeedWriteOutcome::Stored);
        assert_eq!(store.get(id).unwrap(), Some(need.clone()));
        assert_eq!(store.load_all().unwrap(), vec![need.clone()]);

        need.generation = 1;
        need.state = StandingNeedState::Paused;
        assert_eq!(
            store.put(&need).unwrap().1,
            StandingNeedWriteOutcome::Updated
        );
        let mut stale = need.clone();
        stale.generation = 0;
        assert_eq!(
            store.put(&stale).unwrap().1,
            StandingNeedWriteOutcome::StaleGeneration
        );
        let mut conflict = need.clone();
        conflict.state = StandingNeedState::Retired;
        assert_eq!(
            store.put(&conflict).unwrap().1,
            StandingNeedWriteOutcome::GenerationConflict
        );
    }

    #[test]
    fn minimal_views_rebuild_same_roots_and_do_not_become_source_of_record() {
        let need = need();
        let mapping = MappingKernelCid::from_bytes([20; 32]);
        let resolution = resolution(mapping);
        let mapping_record = MappingViewRecord {
            kernel: mapping,
            envelope: ObjectCid::from_bytes([21; 32]),
            disclosure: DisclosureClass::Public,
        };
        let first = MinimalKnowledgeViews::rebuild(
            std::slice::from_ref(&need),
            std::slice::from_ref(&resolution),
            std::slice::from_ref(&mapping_record),
        )
        .unwrap();
        let restarted =
            MinimalKnowledgeViews::rebuild(&[need], &[resolution], &[mapping_record]).unwrap();
        assert_eq!(first.snapshot(), restarted.snapshot());
        assert_eq!(first.standing_needs_for(&reference(1)).len(), 1);
        assert_eq!(first.resolutions_for(&reference(1)).len(), 1);
        assert!(first.mapping(mapping).is_some());
        assert_eq!(first.adopted_targets(mapping), 1);
        assert_eq!(
            first.snapshot().unwrap().resolution_reducer_version,
            RESOLUTION_REDUCER_VERSION
        );
    }

    #[cfg(feature = "storage")]
    #[test]
    fn redb_backend_reopens_with_same_need() {
        let path = std::env::temp_dir().join(format!(
            "onebrain-standing-need-{}-{}.redb",
            std::process::id(),
            u64::from_le_bytes([7; 8])
        ));
        let need = need();
        let id = need.id().unwrap();
        {
            let backend = RedbStandingNeedBackend::open(&path).unwrap();
            let store = StandingNeedStore::new(backend);
            store.put(&need).unwrap();
        }
        {
            let backend = RedbStandingNeedBackend::open(&path).unwrap();
            let store = StandingNeedStore::new(backend);
            assert_eq!(store.get(id).unwrap(), Some(need));
        }
        std::fs::remove_file(path).unwrap();
    }
}
