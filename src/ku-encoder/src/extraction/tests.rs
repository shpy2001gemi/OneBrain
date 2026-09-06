use super::compiler::{compile, exact_number, logical};
use super::*;
use ku_core::foundation::semantic_content::{normalize_semantic_content, SEMANTIC_CONTENT_PROFILE};
use serde_json::{json, Value};

fn budget() -> WorkBudget {
    WorkBudget::new(
        1_000_000,
        Duration::from_secs(120),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap()
}
fn corpus() -> Value {
    serde_json::from_str(include_str!(
        "../../../../docs/specs/vnext/ku-encoder-v1/corpus.json"
    ))
    .unwrap()
}

#[test]
fn native_compiler_matches_all_reviewed_corpus_oracles() {
    let corpus = corpus();
    for row in corpus["cases"].as_array().unwrap() {
        let actual = match compile(
            &row["context"],
            &row["candidate"],
            &row["resolution"],
            &mut budget(),
        ) {
            Ok(Some(sem)) => json!({"status":"compilable","sem":logical(&sem)}),
            Ok(None) => json!({"status":"needs_resolution"}),
            Err(e) => json!({"error":e.0}),
        };
        assert_eq!(actual, row["expected"], "{}", row["id"]);
    }
}

#[test]
fn strict_parser_rejects_duplicate_float_depth_and_truncation_without_payload_errors() {
    for raw in [
        b"{\"a\":1,\"a\":2}".as_slice(),
        b"1.5",
        b"NaN",
        b"{\"PRIVATE",
        b"\xff",
    ] {
        let e = parse(raw, &mut budget()).unwrap_err();
        assert_eq!(e.0, "invalid_json");
        assert!(!format!("{e:?}").contains("PRIVATE"));
    }
    let deep = format!("{}0{}", "[".repeat(33), "]".repeat(33));
    assert_eq!(
        parse(deep.as_bytes(), &mut budget()).unwrap_err().0,
        "json_depth"
    );
    assert_eq!(
        parse(&vec![b' '; 1_048_577], &mut budget()).unwrap_err().0,
        "payload_bytes"
    );
}

#[test]
fn native_context_hash_matches_python_utf8_sorted_json() {
    for row in corpus()["cases"].as_array().unwrap() {
        assert_eq!(
            hash(&row["context"]).unwrap(),
            row["candidate"]["context_sha256"].as_str().unwrap()
        );
    }
}

#[test]
fn exact_decimal_reduces_before_integer_bounds_without_floats() {
    let a = exact_number("273.15").unwrap();
    assert_eq!((a.numerator(), a.denominator()), (5463, 20));
    assert_eq!(
        exact_number("100000000000000000000000000000/100000000000000000000000000000")
            .unwrap()
            .numerator(),
        1
    );
    assert_eq!(
        exact_number("-9223372036854775808").unwrap().numerator(),
        i64::MIN
    );
    assert_eq!(
        exact_number("0.00000000000000000000000000000000000000000000000000000000000000")
            .unwrap()
            .numerator(),
        0
    );
    for x in [
        "1e3",
        "1,5",
        "1/0",
        "01",
        "9223372036854775808",
        "1/18446744073709551616",
    ] {
        assert!(exact_number(x).is_err(), "{x}");
    }
}

#[test]
fn private_provenance_changes_do_not_change_product_semantic_identity() {
    let corpus = corpus();
    let mut row = corpus["cases"][0].clone();
    let first = compile(
        &row["context"],
        &row["candidate"],
        &row["resolution"],
        &mut budget(),
    )
    .unwrap()
    .unwrap();
    row["context"]["source_ref"] = "ff".repeat(32).into();
    let hash = hash(&row["context"]).unwrap();
    row["candidate"]["context_sha256"] = hash.clone().into();
    row["resolution"]["context_sha256"] = hash.into();
    let second = compile(
        &row["context"],
        &row["candidate"],
        &row["resolution"],
        &mut budget(),
    )
    .unwrap()
    .unwrap();
    assert_ne!(
        first.canonical_bytes().unwrap(),
        second.canonical_bytes().unwrap()
    );
    let a = normalize_semantic_content(&first, SEMANTIC_CONTENT_PROFILE).unwrap();
    let b = normalize_semantic_content(&second, SEMANTIC_CONTENT_PROFILE).unwrap();
    assert_eq!(a.cid, b.cid);
    assert_eq!(a.canonical_bytes, b.canonical_bytes);
    assert_ne!(a.private_input_bytes, b.private_input_bytes);
}

#[test]
fn schema_and_span_walks_obey_work_cancellation_and_deadline() {
    let row = corpus()["cases"][0].clone();
    let mut b =
        WorkBudget::new(1, Duration::from_secs(1), Arc::new(AtomicBool::new(false))).unwrap();
    assert_eq!(
        compile(
            &row["context"],
            &row["candidate"],
            &row["resolution"],
            &mut b
        )
        .unwrap_err()
        .0,
        "resource"
    );
    for (cancel, timeout, reason) in [
        (true, Duration::from_secs(1), "canceled"),
        (false, Duration::ZERO, "deadline"),
    ] {
        let mut b = WorkBudget::new(1000, timeout, Arc::new(AtomicBool::new(cancel))).unwrap();
        assert_eq!(parse(b"{}", &mut b).unwrap_err().0, reason);
    }
}
