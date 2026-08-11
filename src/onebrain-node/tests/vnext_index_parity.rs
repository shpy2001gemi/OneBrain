use ku_core::core_dna::{CoreDna, CoreDnaHeader, Instruction};
use ku_core::foundation::{
    AcceptedRecordEntry, CanonicalValue, DisclosureClass, KnowledgeObjectEnvelope, ObjectKind,
    ObjectReference, ReservedDomain, ResourceProfile, SchemaVersion, StoredRecordKind,
};
use ku_core::{Epigenetics, KuRuntime};
use ku_kql::storage::{KuStorage, StorageError};
use onebrain_node::{
    AcceptedRecordScan, DerivedIndexError, DerivedIndexOpenState, VNextDerivedIndexManager,
};

struct Records(Vec<AcceptedRecordEntry>);

impl AcceptedRecordScan for Records {
    fn accepted_records(&self) -> Result<Vec<AcceptedRecordEntry>, DerivedIndexError> {
        Ok(self.0.clone())
    }
}

#[test]
fn frozen_projection_vector_and_rebuild_are_root_stable() {
    let vector: serde_json::Value = serde_json::from_str(include_str!(
        "../../test-vectors/vnext/base-v1-derived-projection-v1.json"
    ))
    .unwrap();
    assert_eq!(vector["format"], "onebrain/base-v1-derived-projection/1");
    assert_eq!(vector["object_mappings"].as_array().unwrap().len(), 23);
    assert_eq!(vector["event_mappings"].as_array().unwrap().len(), 7);

    let source = object_source();
    let temp = tempfile::tempdir().unwrap();
    let manager = VNextDerivedIndexManager::new(temp.path()).unwrap();
    let (state, first) = manager.open_or_rebuild(&source);
    assert_eq!(state, DerivedIndexOpenState::Rebuilt, "{first:?}");
    let first = first.unwrap();
    assert_eq!(first.accepted_record_count, 1);
    assert_ne!(first.graph_root, [0; 32]);
    assert_ne!(first.secondary_root, [0; 32]);
    assert_eq!(manager.verify_parity(&source).unwrap(), first);

    let generation = manager.current_generation_path().unwrap();
    let index = generation.join("index.json");
    let original = std::fs::read(&index).unwrap();

    let mut unknown: serde_json::Value = serde_json::from_slice(&original).unwrap();
    unknown["graph_rows"][0]["mapping_id"] = serde_json::json!("unknown/mapping");
    std::fs::write(&index, serde_json::to_vec(&unknown).unwrap()).unwrap();
    assert!(matches!(
        manager.verify_parity(&source),
        Err(DerivedIndexError::Parity)
    ));
    assert_eq!(
        manager.open_or_rebuild(&source).0,
        DerivedIndexOpenState::Rebuilt
    );
    assert_eq!(manager.verify_parity(&source).unwrap(), first);

    let mut missing: serde_json::Value = serde_json::from_slice(&original).unwrap();
    missing["graph_rows"].as_array_mut().unwrap().clear();
    std::fs::write(&index, serde_json::to_vec(&missing).unwrap()).unwrap();
    assert!(manager.verify_parity(&source).is_err());
    assert_eq!(
        manager.open_or_rebuild(&source).0,
        DerivedIndexOpenState::Rebuilt
    );

    let mut extra: serde_json::Value = serde_json::from_slice(&original).unwrap();
    let duplicate = extra["graph_rows"][0].clone();
    extra["graph_rows"].as_array_mut().unwrap().push(duplicate);
    std::fs::write(&index, serde_json::to_vec(&extra).unwrap()).unwrap();
    assert!(manager.verify_parity(&source).is_err());
    assert_eq!(
        manager.open_or_rebuild(&source).0,
        DerivedIndexOpenState::Rebuilt
    );

    std::fs::write(&index, b"{").unwrap();
    assert_eq!(
        manager.open_or_rebuild(&source).0,
        DerivedIndexOpenState::Rebuilt
    );
    std::fs::remove_file(&index).unwrap();
    assert_eq!(
        manager.open_or_rebuild(&source).0,
        DerivedIndexOpenState::Rebuilt
    );
    assert_eq!(manager.verify_parity(&source).unwrap(), first);
}

#[test]
fn canonical_cid_corruption_fails_closed_before_influencing_a_root() {
    let mut source = object_source();
    source.0[0].claimed_cid[0] ^= 1;
    let temp = tempfile::tempdir().unwrap();
    let manager = VNextDerivedIndexManager::new(temp.path()).unwrap();
    assert!(matches!(
        manager.rebuild(&source),
        Err(DerivedIndexError::CanonicalCidMismatch)
    ));
    assert!(!temp.path().join("current.json").exists());
}

#[test]
fn base_mode_fences_every_legacy_write_entry_point() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("legacy.redb");
    let base = KuStorage::open_base_read_only(&path).unwrap();
    let ku = test_ku();
    assert!(matches!(base.put(&ku), Err(StorageError::LegacyReadOnly)));
    assert!(matches!(
        base.update_epi(&ku.cid, &ku.epi),
        Err(StorageError::LegacyReadOnly)
    ));
    assert!(matches!(
        base.delete(&ku.cid),
        Err(StorageError::LegacyReadOnly)
    ));
    assert!(matches!(
        base.scan_migration_evidence(),
        Err(StorageError::LegacyReadOnly)
    ));

    drop(base);
    let migration = KuStorage::open_migration_evidence(&path).unwrap();
    assert!(migration.scan_migration_evidence().unwrap().is_empty());
    assert!(matches!(
        migration.put(&ku),
        Err(StorageError::LegacyReadOnly)
    ));
}

fn object_source() -> Records {
    let mut envelope = KnowledgeObjectEnvelope::new(
        ObjectKind(2),
        SchemaVersion::new(1, 0),
        DisclosureClass::Public,
        CanonicalValue::Map(vec![(
            0,
            CanonicalValue::Text("nguồn canonical không chọn nhánh thắng".into()),
        )]),
    );
    envelope.references = vec![ObjectReference::new(22, [0x44; 32])];
    let (bytes, cid) = envelope.encode(ResourceProfile::ObjectV1).unwrap();
    assert_eq!(ReservedDomain::Object.digest(&bytes), cid.into_bytes());
    Records(vec![AcceptedRecordEntry {
        record_kind: StoredRecordKind::Object,
        claimed_cid: cid.into_bytes(),
        canonical_bytes: bytes,
    }])
}

fn test_ku() -> KuRuntime {
    let dna = CoreDna {
        header: CoreDnaHeader {
            version: 2,
            gene_type: 0,
            has_concept_table: false,
        },
        concept_table: Vec::new(),
        instructions: vec![Instruction::Certainty { level: 9000 }],
    };
    let mut ku = KuRuntime::from_dna(dna).unwrap();
    ku.epi = Epigenetics::with_trust(8000, 8000);
    ku
}
