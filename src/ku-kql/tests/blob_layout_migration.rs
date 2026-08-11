#![cfg(feature = "storage")]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use ku_core::blob_store::{BlobCid, BlobMeta, BlobType};
use ku_kql::blob_layout::{blob_relative_dir, migrate_blob_layout_v2, BLOB_LAYOUT_VERSION};
use ku_kql::blob_storage::{BlobStorage, BlobStorageError};

#[test]
fn full_cid_paths_separate_legacy_prefix_collisions() {
    let mut left = [0u8; 34];
    left[..4].copy_from_slice(&[1, BlobType::Raw as u8, 0xaa, 0xbb]);
    left[4] = 0x10;
    let mut right = left;
    right[4] = 0x20;
    let left = BlobCid(left);
    let right = BlobCid(right);

    assert_eq!(left.short_hex(), right.short_hex());
    assert_ne!(blob_relative_dir(&left), blob_relative_dir(&right));
    assert_eq!(BLOB_LAYOUT_VERSION, 2);

    let left_path = blob_relative_dir(&left);
    let components: Vec<_> = left_path
        .iter()
        .map(|part| part.to_string_lossy().into_owned())
        .collect();
    assert_eq!(components[0], "v2");
    assert_eq!(components[1], "aa");
    assert_eq!(components[2], "bb");
    assert_eq!(components[3], left.to_hex());
    assert_eq!(components[3].len(), 68);
    assert_eq!(
        blob_relative_dir(&right).file_name().unwrap(),
        right.to_hex().as_str()
    );
}

#[test]
fn valid_v1_directory_migrates_and_rerun_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let data = b"legacy blob split across chunks";
    let cid = BlobCid::from_content(BlobType::Raw, data);
    let meta = meta_for(&cid, data, 7);
    let legacy = legacy_dir(temp.path(), &cid);
    write_chunks(&legacy, data, meta.chunk_size as usize);

    let report = migrate_blob_layout_v2(temp.path(), std::slice::from_ref(&meta)).unwrap();
    assert_eq!(report.migrated, 1);
    assert_eq!(report.already_v2, 0);
    assert!(report.collision_groups.is_empty());
    assert!(report.corrupt_cids.is_empty());
    assert!(!legacy.exists());

    let v2 = temp.path().join(blob_relative_dir(&cid));
    assert_eq!(read_chunks(&v2, meta.chunk_count), data);

    let rerun = migrate_blob_layout_v2(temp.path(), &[meta]).unwrap();
    assert_eq!(rerun.migrated, 0);
    assert_eq!(rerun.already_v2, 1);
    assert_eq!(read_chunks(&v2, 5), data);
}

#[test]
fn already_v2_blob_is_verified_without_rewrite() {
    let temp = tempfile::tempdir().unwrap();
    let data = b"already migrated";
    let cid = BlobCid::from_content(BlobType::Document, data);
    let meta = meta_for(&cid, data, 5);
    let chunk_count = meta.chunk_count;
    let v2 = temp.path().join(blob_relative_dir(&cid));
    write_chunks(&v2, data, meta.chunk_size as usize);

    let report = migrate_blob_layout_v2(temp.path(), &[meta]).unwrap();
    assert_eq!(report.migrated, 0);
    assert_eq!(report.already_v2, 1);
    assert_eq!(read_chunks(&v2, chunk_count), data);
}

#[test]
fn colliding_prefixes_are_allowed_after_both_blobs_are_v2() {
    let temp = tempfile::tempdir().unwrap();
    let (left_data, right_data) = find_digest_prefix_collision();
    let left = BlobCid::from_content(BlobType::Raw, &left_data);
    let right = BlobCid::from_content(BlobType::Raw, &right_data);
    assert_eq!(left.short_hex(), right.short_hex());

    let left_meta = meta_for(&left, &left_data, 3);
    let right_meta = meta_for(&right, &right_data, 3);
    write_chunks(
        &temp.path().join(blob_relative_dir(&left)),
        &left_data,
        left_meta.chunk_size as usize,
    );
    write_chunks(
        &temp.path().join(blob_relative_dir(&right)),
        &right_data,
        right_meta.chunk_size as usize,
    );

    let report = migrate_blob_layout_v2(temp.path(), &[left_meta, right_meta]).unwrap();
    assert_eq!(report.already_v2, 2);
    assert!(report.collision_groups.is_empty());
}

#[test]
fn legacy_prefix_collision_blocks_without_touching_v1() {
    let temp = tempfile::tempdir().unwrap();
    let mut left = [0u8; 34];
    left[..4].copy_from_slice(&[1, BlobType::Raw as u8, 0x12, 0x34]);
    left[4] = 0x56;
    let mut right = left;
    right[4] = 0x78;
    let left = BlobCid(left);
    let right = BlobCid(right);
    let legacy = legacy_dir(temp.path(), &left);
    fs::create_dir_all(&legacy).unwrap();
    fs::write(legacy.join("sentinel"), b"preserve me").unwrap();

    let error = migrate_blob_layout_v2(
        temp.path(),
        &[meta_for(&left, b"left", 4), meta_for(&right, b"right", 4)],
    )
    .unwrap_err();
    let report = blocked_report(error);
    assert_eq!(report.collision_groups, vec![left.short_hex()]);
    assert!(legacy.join("sentinel").exists());
    assert!(!temp.path().join(blob_relative_dir(&left)).exists());
    assert!(!temp.path().join(blob_relative_dir(&right)).exists());
}

#[test]
fn corrupt_legacy_chunks_block_without_moving_data() {
    let temp = tempfile::tempdir().unwrap();
    let data = b"expected bytes";
    let cid = BlobCid::from_content(BlobType::Raw, data);
    let meta = meta_for(&cid, data, 4);
    let legacy = legacy_dir(temp.path(), &cid);
    write_chunks(&legacy, b"corrupted bytes", meta.chunk_size as usize);

    let report = blocked_report(migrate_blob_layout_v2(temp.path(), &[meta]).unwrap_err());
    assert_eq!(report.corrupt_cids, vec![cid]);
    assert!(legacy.exists());
    assert!(!temp.path().join(blob_relative_dir(&cid)).exists());
}

#[test]
fn declared_type_mismatch_blocks_as_a_corrupt_typed_cid() {
    let temp = tempfile::tempdir().unwrap();
    let data = b"typed content";
    let cid = BlobCid::from_content(BlobType::Raw, data);
    let mut meta = meta_for(&cid, data, 4);
    meta.blob_type = BlobType::Document as u8;
    let legacy = legacy_dir(temp.path(), &cid);
    write_chunks(&legacy, data, meta.chunk_size as usize);

    let report = blocked_report(migrate_blob_layout_v2(temp.path(), &[meta]).unwrap_err());
    assert_eq!(report.corrupt_cids, vec![cid]);
    assert!(legacy.exists());
    assert!(!temp.path().join(blob_relative_dir(&cid)).exists());
}

#[test]
fn interrupted_migrating_directory_is_verified_and_published() {
    let temp = tempfile::tempdir().unwrap();
    let data = b"resume staged migration";
    let cid = BlobCid::from_content(BlobType::Audio, data);
    let meta = meta_for(&cid, data, 6);
    let final_dir = temp.path().join(blob_relative_dir(&cid));
    let staging = migrating_dir(&final_dir);
    write_chunks(&staging, data, meta.chunk_size as usize);

    let report = migrate_blob_layout_v2(temp.path(), &[meta]).unwrap();
    assert_eq!(report.migrated, 1);
    assert_eq!(report.already_v2, 0);
    assert!(!staging.exists());
    assert_eq!(read_chunks(&final_dir, 4), data);
}

#[test]
fn storage_reopen_migrates_legacy_spill_and_reads_from_v2() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("blob.redb");
    let data = vec![0x5a; 1024 * 1024 + 1];
    let meta = {
        let storage = BlobStorage::open(&db_path).unwrap();
        storage
            .store_bytes("large.bin", &data, BlobType::Raw)
            .unwrap()
    };
    let cid = BlobCid::from_hex(&meta.blob_cid_hex).unwrap();
    let root = temp.path().join("blobs");
    let v2 = root.join(blob_relative_dir(&cid));
    let legacy = legacy_dir(&root, &cid);
    assert!(v2.exists());
    assert!(!legacy.exists());
    fs::rename(&v2, &legacy).unwrap();

    let reopened = BlobStorage::open(&db_path).unwrap();
    assert!(!legacy.exists());
    assert!(v2.exists());
    assert_eq!(reopened.read_full_blob(&cid).unwrap(), data);
}

fn meta_for(cid: &BlobCid, data: &[u8], chunk_size: u32) -> BlobMeta {
    BlobMeta {
        meta_version: 2,
        blob_cid_hex: cid.to_hex(),
        original_name: "fixture.bin".into(),
        mime_type: "application/octet-stream".into(),
        total_size: data.len() as u64,
        chunk_count: data.len().div_ceil(chunk_size as usize) as u32,
        chunk_size,
        blob_type: cid.0[1],
        created_at: 0,
        blake3_hex: hex(cid.blake3_hash()),
        referencing_kus: Vec::new(),
        pinned: false,
        storage_mode: "filesystem".into(),
        chunk_blake3: data
            .chunks(chunk_size as usize)
            .map(|chunk| hex(blake3::hash(chunk).as_bytes()))
            .collect(),
    }
}

fn legacy_dir(root: &Path, cid: &BlobCid) -> PathBuf {
    root.join(cid.short_hex())
}

fn migrating_dir(final_dir: &Path) -> PathBuf {
    final_dir.with_extension("migrating")
}

fn write_chunks(dir: &Path, data: &[u8], chunk_size: usize) {
    fs::create_dir_all(dir).unwrap();
    for (index, chunk) in data.chunks(chunk_size).enumerate() {
        fs::write(dir.join(format!("chunk_{index:04}.bin")), chunk).unwrap();
    }
}

fn read_chunks(dir: &Path, chunk_count: u32) -> Vec<u8> {
    let mut data = Vec::new();
    for index in 0..chunk_count {
        let path = dir.join(format!("chunk_{index:04}.bin"));
        if !path.exists() {
            break;
        }
        data.extend(fs::read(path).unwrap());
    }
    data
}

fn blocked_report(error: BlobStorageError) -> ku_kql::blob_layout::BlobLayoutMigrationReport {
    match error {
        BlobStorageError::MigrationBlocked(report) => report,
        other => panic!("expected MigrationBlocked, got {other}"),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn find_digest_prefix_collision() -> (Vec<u8>, Vec<u8>) {
    let mut seen = BTreeMap::new();
    for counter in 0u32..=u32::MAX {
        let data = counter.to_be_bytes().to_vec();
        let digest = blake3::hash(&data);
        let prefix = [digest.as_bytes()[0], digest.as_bytes()[1]];
        if let Some(previous) = seen.insert(prefix, data.clone()) {
            return (previous, data);
        }
    }
    unreachable!("a 16-bit prefix collision must exist")
}
