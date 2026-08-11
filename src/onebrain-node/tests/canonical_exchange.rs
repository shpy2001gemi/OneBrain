use std::io::Cursor;

use ku_core::core_dna::{CoreDna, CoreDnaHeader, Instruction};
use ku_core::foundation::schema_registry::OBJECT_KIND_SEMANTIC_KERNEL;
use ku_core::foundation::{
    CanonicalValue, DisclosureClass, KnowledgeObjectEnvelope, ObjectKind, ResourceProfile,
    SchemaVersion, StoredRecordKind,
};
use ku_core::KuRuntime;
use onebrain_node::{
    read_canonical_exchange, truncate_preview, write_canonical_exchange, BaseExchangeEntryV1,
    ExchangeError,
};

#[test]
fn unicode_preview_preserves_vietnamese_cjk_and_emoji_clusters() {
    let clusters = ["ế", "e\u{302}\u{301}", "漢", "👩🏽‍💻", "🇻🇳", "👨‍👩‍👧‍👦"];
    for cluster in clusters {
        assert_eq!(truncate_preview(cluster, 0), "");
        assert_eq!(truncate_preview(cluster, 1), cluster);
        for limit in [77, 80] {
            let exact = cluster.repeat(limit);
            assert_eq!(truncate_preview(&exact, limit), exact);
            let truncated = truncate_preview(&cluster.repeat(limit + 1), limit);
            assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
            assert!(!truncated.contains('\u{fffd}'));
        }
    }
}

#[test]
fn writer_is_deterministic_and_reader_preserves_exact_bytes() {
    let first = public_object(0x11, "Tiếng Việt 👩🏽‍💻");
    let second = public_object(0x22, "漢字");
    let legacy = legacy_evidence();
    let mut left = Vec::new();
    let receipt =
        write_canonical_exchange(&[legacy.clone(), second.clone(), first.clone()], &mut left)
            .unwrap();
    let mut right = Vec::new();
    let reordered_receipt =
        write_canonical_exchange(&[first.clone(), legacy.clone(), second.clone()], &mut right)
            .unwrap();
    assert_eq!(left, right);
    assert_eq!(receipt, reordered_receipt);

    let decoded = read_canonical_exchange(Cursor::new(&left)).unwrap();
    let mut expected = vec![first, second, legacy];
    expected.sort_by_key(|entry| match entry {
        BaseExchangeEntryV1::VNextPublic { kind, cid, .. } => (*kind as u8, *cid),
        BaseExchangeEntryV1::LegacyReadOnlyEvidence { cid, .. } => (u8::MAX, *cid),
    });
    assert_eq!(decoded, expected);
}

#[test]
fn writer_rejects_cid_mismatch_duplicates_and_private_class() {
    let mut wrong = public_object(1, "exact");
    if let BaseExchangeEntryV1::VNextPublic { cid, .. } = &mut wrong {
        cid[0] ^= 1;
    }
    assert!(matches!(
        write_canonical_exchange(&[wrong], Vec::new()),
        Err(ExchangeError::CidMismatch)
    ));

    let entry = public_object(2, "duplicate");
    assert!(matches!(
        write_canonical_exchange(&[entry.clone(), entry], Vec::new()),
        Err(ExchangeError::DuplicateCid)
    ));

    assert!(matches!(
        write_canonical_exchange(&[private_object()], Vec::new()),
        Err(ExchangeError::PrivateClass)
    ));
}

#[test]
fn reader_rejects_malformed_hex_unknown_kind_version_and_trailing_bytes() {
    let bytes = exchange_bytes(&[public_object(3, "valid")]);
    let text = String::from_utf8(bytes.clone()).unwrap();

    let malformed = text.replacen("\"cid\":\"", "\"cid\":\"g", 1);
    assert!(matches!(
        read_canonical_exchange(Cursor::new(malformed.as_bytes())),
        Err(ExchangeError::Malformed)
    ));

    let unknown_kind = text.replacen("\"kind\":1", "\"kind\":99", 1);
    assert!(matches!(
        read_canonical_exchange(Cursor::new(unknown_kind.as_bytes())),
        Err(ExchangeError::UnknownVersionOrKind)
    ));

    let unknown_version = text.replacen("\"version\":1", "\"version\":2", 1);
    assert!(matches!(
        read_canonical_exchange(Cursor::new(unknown_version.as_bytes())),
        Err(ExchangeError::UnknownVersionOrKind)
    ));

    let reordered_fields = text.replacen(
        "{\"version\":1,\"class\":\"vnext-public\"",
        "{\"class\":\"vnext-public\",\"version\":1",
        1,
    );
    assert!(matches!(
        read_canonical_exchange(Cursor::new(reordered_fields.as_bytes())),
        Err(ExchangeError::NonCanonicalEncoding)
    ));

    let mut trailing = bytes;
    trailing.extend_from_slice(b"trailing");
    assert!(matches!(
        read_canonical_exchange(Cursor::new(trailing)),
        Err(ExchangeError::TrailingBytes)
    ));
}

#[test]
fn reader_rejects_duplicate_record_and_one_byte_corruption() {
    let entry = public_object(4, "root-bound");
    let bytes = exchange_bytes(&[entry]);
    let body = String::from_utf8(bytes.clone()).unwrap();
    let mut lines = body.lines().map(str::to_owned).collect::<Vec<_>>();
    lines.insert(2, lines[1].clone());
    let duplicate = format!("{}\n", lines.join("\n"));
    assert!(matches!(
        read_canonical_exchange(Cursor::new(duplicate.as_bytes())),
        Err(ExchangeError::DuplicateCid)
    ));

    let mut corrupt = bytes;
    let position = corrupt
        .windows(b"canonical_hex".len())
        .position(|window| window == b"canonical_hex")
        .unwrap()
        + b"canonical_hex".len()
        + 3;
    corrupt[position] = if corrupt[position] == b'0' {
        b'1'
    } else {
        b'0'
    };
    assert!(read_canonical_exchange(Cursor::new(corrupt)).is_err());
}

fn public_object(marker: u8, text: &str) -> BaseExchangeEntryV1 {
    object_entry(marker, text, DisclosureClass::Public)
}

fn private_object() -> BaseExchangeEntryV1 {
    object_entry(0x55, "private", DisclosureClass::LocalOnly)
}

fn object_entry(marker: u8, text: &str, disclosure: DisclosureClass) -> BaseExchangeEntryV1 {
    let object = KnowledgeObjectEnvelope::new(
        ObjectKind(OBJECT_KIND_SEMANTIC_KERNEL),
        SchemaVersion::new(1, 0),
        disclosure,
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(marker as u64)),
            (1, CanonicalValue::Text(text.to_owned())),
        ]),
    );
    let (canonical_bytes, cid) = object.encode(ResourceProfile::ObjectV1).unwrap();
    BaseExchangeEntryV1::VNextPublic {
        kind: StoredRecordKind::Object,
        cid: cid.into_bytes(),
        canonical_bytes,
    }
}

fn legacy_evidence() -> BaseExchangeEntryV1 {
    let dna = CoreDna {
        header: CoreDnaHeader {
            version: 2,
            gene_type: 0,
            has_concept_table: false,
        },
        concept_table: Vec::new(),
        instructions: vec![Instruction::Certainty { level: 9000 }],
    };
    let ku = KuRuntime::from_dna(dna).unwrap();
    assert_eq!(ku.cid, *blake3::hash(&ku.wire_bytes).as_bytes());
    BaseExchangeEntryV1::LegacyReadOnlyEvidence {
        cid: ku.cid,
        wire_bytes: ku.wire_bytes,
        epigenetics_json: serde_json::to_vec(&ku.epi).unwrap(),
    }
}

fn exchange_bytes(entries: &[BaseExchangeEntryV1]) -> Vec<u8> {
    let mut output = Vec::new();
    write_canonical_exchange(entries, &mut output).unwrap();
    assert!(output.starts_with(b"OBXV1\n"));
    output
}
