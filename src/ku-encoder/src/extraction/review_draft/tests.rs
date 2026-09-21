use super::*;
#[test]
fn review_draft_wire_constraints_only_offer_source_quotes_and_keep_property_order() {
    let request = TaskRequest {
        kind: TaskKind::Draft,
        data: json!({"SOURCE":"Đèn thường sáng."}),
        deadline: Duration::from_secs(30),
    };
    let raw = request.wire_schema().unwrap();
    assert!(raw.find("\"subject\"").unwrap() < raw.find("\"predicate\"").unwrap());
    let schema: Value = serde_json::from_str(&raw).unwrap();
    let options = schema["properties"]["statements"]["items"]["properties"]["subject"]["enum"]
        .as_array()
        .unwrap();
    assert!(options.contains(&json!("Đèn")));
    assert!(!options.contains(&json!("Bưu kiện")));
    assert!(options
        .iter()
        .all(|q| "Đèn thường sáng.".contains(q.as_str().unwrap())));
}
fn budget() -> WorkBudget {
    WorkBudget::new(
        1_000_000,
        Duration::from_secs(30),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap()
}
fn draft() -> Value {
    json!({"statements":[{"subject":"Đèn","predicate":"sáng","arguments":[],"frequency":["thường"],"negation":[],"condition":[],"time":[],"location":[],"modality":[],"approximation":[],"numbers":[],"relations":[],"evidence":"Đèn thường sáng."}],"unresolved":[]})
}
#[test]
fn review_draft_role_coverage_and_atomic_repair() {
    let mut d = draft();
    assert!(validate("Đèn thường sáng.", &d, &mut budget())
        .unwrap()
        .clear());
    d["statements"][0]["frequency"] = json!([]);
    assert_eq!(
        validate("Đèn thường sáng.", &d, &mut budget())
            .unwrap()
            .uncovered,
        vec!["thường"]
    );
    let review = json!({"edits":[{"statement":0,"field":"frequency","before":"","after":"thường"}],"missing":[],"unresolved":[]});
    assert_eq!(
        apply_review("Đèn thường sáng.", &d, &review, &mut budget()).unwrap(),
        draft()
    );
    let mut fabricated = review.clone();
    fabricated["edits"][0]["after"] = json!("always");
    assert!(apply_review("Đèn thường sáng.", &d, &fabricated, &mut budget()).is_err());
    assert!(d["statements"][0]["frequency"]
        .as_array()
        .unwrap()
        .is_empty());
}
#[test]
fn review_draft_rejects_coverage_loss_and_stale_edit() {
    let deletion = json!({"edits":[{"statement":0,"field":"frequency","before":"thường","after":""}],"missing":[],"unresolved":[]});
    assert_eq!(
        apply_review("Đèn thường sáng.", &draft(), &deletion, &mut budget())
            .unwrap_err()
            .0,
        "edit_revalidation"
    );
    let mut stale = deletion.clone();
    stale["edits"][0]["before"] = json!("luôn");
    assert!(apply_review("Đèn thường sáng.", &draft(), &stale, &mut budget()).is_err());
}

#[test]
fn review_draft_empty_array_edit_is_inert_but_never_accepts_a_stale_value() {
    let noop = json!({"edits":[{"statement":0,"field":"arguments","before":"","after":""}],"missing":[],"unresolved":[]});
    assert_eq!(
        apply_review("Đèn thường sáng.", &draft(), &noop, &mut budget()).unwrap(),
        draft()
    );
    let mut stale = noop;
    stale["edits"][0]["before"] = json!("không tồn tại");
    assert!(apply_review("Đèn thường sáng.", &draft(), &stale, &mut budget()).is_err());
}

#[test]
fn review_draft_repeated_inert_edits_do_not_mask_stale_or_mutating_conflicts() {
    let edit = json!({"statement":0,"field":"frequency","before":"thường","after":"thường"});
    let review = json!({"edits":[edit.clone(),edit],"missing":[],"unresolved":[]});
    assert_eq!(
        apply_review("Đèn thường sáng.", &draft(), &review, &mut budget()).unwrap(),
        draft()
    );
    let mut stale = review;
    stale["edits"][1]["before"] = json!("luôn");
    stale["edits"][1]["after"] = json!("luôn");
    assert!(apply_review("Đèn thường sáng.", &draft(), &stale, &mut budget()).is_err());
}

#[test]
fn review_draft_number_stage_only_fills_incomplete_quantities() {
    let mut d = draft();
    d["statements"][0]["numbers"] =
        json!([{"value_quote":"12","unit_quote":"kg","counted_entity_quote":""}]);
    assert!(!needs_number_task("Vật nặng 12kg.", &d).unwrap());
    assert!(needs_number_task("Vật nặng 12kg và có 5 hạt.", &d).unwrap());
    d["statements"][0]["numbers"][0]["unit_quote"] = json!("");
    assert!(needs_number_task("Vật nặng 12kg.", &d).unwrap());
    d["statements"][0]["numbers"][0]["unit_quote"] = json!("kg");
    d["statements"][0]["numbers"][0]["counted_entity_quote"] = json!("Vật");
    assert!(needs_number_task("Vật nặng 12kg.", &d).unwrap());
    assert!(validate("Vật nặng 12kg.", &d, &mut budget())
        .unwrap()
        .issues
        .iter()
        .any(|e| e.contains("ambiguous_quantity_kind")));
}

#[test]
fn review_draft_admission_is_atomic_and_restart_does_not_reset_window_cap() {
    let source = "Đèn thường sáng.";
    let mut job = DraftJob::new(source).unwrap();
    let (i, mut task) = job.next(source).unwrap().unwrap();
    let before = serde_json::to_value(&job).unwrap();
    task.deadline = Duration::from_secs(601);
    assert!(job.reserve(i, &task, 100).is_err());
    assert_eq!(serde_json::to_value(&job).unwrap(), before);
    job.windows[0].calls = 5;
    job.interrupt();
    assert!(job.next(source).is_err());
}

#[test]
fn review_draft_failed_review_preserves_proposal_and_last_good_revision() {
    let source = "Đèn thường sáng.";
    let mut job = DraftJob::new(source).unwrap();
    let (i, t) = job.next(source).unwrap().unwrap();
    job.reserve(i, &t, 100).unwrap();
    job.finish(
        source,
        Ok(serde_json::to_vec(&draft()).unwrap()),
        1,
        "test",
        &mut budget(),
    )
    .unwrap();
    let (i, t) = job.next(source).unwrap().unwrap();
    job.reserve(i, &t, 100).unwrap();
    let invalid = json!({"edits":[{"statement":0,"field":"frequency","before":"luôn","after":"thường"}],"missing":[],"unresolved":[]});
    job.finish(
        source,
        Ok(serde_json::to_vec(&invalid).unwrap()),
        1,
        "reviewer",
        &mut budget(),
    )
    .unwrap();
    assert_eq!(job.state, "needs_review");
    assert_eq!(job.windows[0].revisions.len(), 1);
    assert_eq!(job.windows[0].reviews, vec![invalid]);
}
#[test]
fn review_draft_checkpoint_call_reservation_and_crash_charge() {
    let mut job = DraftJob::new("Đèn thường sáng.").unwrap();
    let (i, task) = job.next("Đèn thường sáng.").unwrap().unwrap();
    job.reserve(i, &task, 100).unwrap();
    assert_eq!(job.charged_ms, 600_000);
    assert!(job.next("Đèn thường sáng.").is_err());
    let mut recovered: DraftJob =
        serde_json::from_slice(&serde_json::to_vec(&job).unwrap()).unwrap();
    recovered.interrupt();
    assert_eq!(recovered.state, "interrupted");
    assert_eq!(recovered.calls, 1);
    assert_eq!(recovered.charged_ms, 600_000);
    job.finish(
        "Đèn thường sáng.",
        Ok(serde_json::to_vec(&draft()).unwrap()),
        20,
        "test",
        &mut budget(),
    )
    .unwrap();
    assert_eq!(job.charged_ms, 20);
    let (i, task) = job.next("Đèn thường sáng.").unwrap().unwrap();
    job.reserve(i, &task, 100).unwrap();
    job.finish(
        "Đèn thường sáng.",
        Ok(br#"{"edits":[],"missing":[],"unresolved":[]}"#.to_vec()),
        20,
        "test",
        &mut budget(),
    )
    .unwrap();
    assert_eq!(job.state, "draft_ready");
    assert!(job.next("Đèn thường sáng.").unwrap().is_none());
}
#[test]
fn review_draft_windows_keep_every_utf8_byte_and_no_silent_truncation() {
    let source = "Đèn sáng. ".repeat(200);
    let job = DraftJob::new(&source).unwrap();
    let joined: String = job
        .windows
        .iter()
        .map(|w| &source[w.start..w.end])
        .collect();
    assert_eq!(source, joined);
    assert!(job.windows.len() > 1);
    assert!(DraftJob::new(&"a".repeat(8193)).is_err());
}

#[test]
fn review_draft_claim_limit_applies_across_windows() {
    let source = "Đèn thường sáng. ".repeat(200);
    let mut job = DraftJob::new(&source).unwrap();
    let mut full = draft();
    full["statements"] = json!(vec![draft()["statements"][0].clone(); 64]);
    job.windows[0].revisions.push(Revision {
        draft: full,
        producer: "test".into(),
        task: TaskKind::Draft,
        validation: Validation::default(),
    });
    job.windows[0].state = "needs_review".into();
    let (i, t) = job.next(&source).unwrap().unwrap();
    assert_eq!(i, 1);
    job.reserve(i, &t, 100).unwrap();
    job.finish(
        &source,
        Ok(serde_json::to_vec(&draft()).unwrap()),
        1,
        "test",
        &mut budget(),
    )
    .unwrap();
    assert!(job.issues.contains(&"claim_bound".into()));
    assert!(job.windows[1].revisions.is_empty());
}

#[test]
fn review_draft_numbers_refine_only_numeric_fields_and_cannot_drop_values() {
    let source = "Thùng chứa 5 táo.";
    let mut d = draft();
    d["statements"][0]["subject"] = json!("Thùng");
    d["statements"][0]["predicate"] = json!("chứa");
    d["statements"][0]["arguments"] = json!(["5 táo"]);
    d["statements"][0]["frequency"] = json!([]);
    d["statements"][0]["evidence"] = json!(source);
    d["statements"][0]["numbers"] =
        json!([{"value_quote":"5","unit_quote":"","counted_entity_quote":""}]);
    let mut job = DraftJob::new(source).unwrap();
    let (i, t) = job.next(source).unwrap().unwrap();
    job.reserve(i, &t, 100).unwrap();
    job.finish(
        source,
        Ok(serde_json::to_vec(&d).unwrap()),
        1,
        "test",
        &mut budget(),
    )
    .unwrap();
    assert!(job.next("changed source").is_err());
    let (i, t) = job.next(source).unwrap().unwrap();
    assert_eq!(t.kind, TaskKind::Numbers);
    let mut rejected = job.clone();
    rejected.reserve(i, &t, 100).unwrap();
    rejected
        .finish(
            source,
            Ok(br#"{"numbers":[]}"#.to_vec()),
            1,
            "test",
            &mut budget(),
        )
        .unwrap();
    assert_eq!(rejected.state, "needs_review");
    assert_eq!(rejected.windows[0].revisions.len(), 1);
    job.reserve(i, &t, 100).unwrap();
    let output = json!({"numbers":[{"statement":0,"value_quote":"5","unit_quote":"","counted_entity_quote":"táo"}]});
    job.finish(
        source,
        Ok(serde_json::to_vec(&output).unwrap()),
        1,
        "test",
        &mut budget(),
    )
    .unwrap();
    let mut expected = d.clone();
    expected["statements"][0]["numbers"][0]["counted_entity_quote"] = json!("táo");
    assert_eq!(job.windows[0].revisions.last().unwrap().draft, expected);
    assert_eq!(job.next(source).unwrap().unwrap().1.kind, TaskKind::Review);
}

#[test]
fn review_draft_supplement_keeps_existing_claims_and_rebases_new_relations() {
    let source = "Đèn thường sáng. Quạt chạy.";
    let mut job = DraftJob::new(source).unwrap();
    let (i, t) = job.next(source).unwrap().unwrap();
    job.reserve(i, &t, 100).unwrap();
    job.finish(
        source,
        Ok(serde_json::to_vec(&draft()).unwrap()),
        1,
        "test",
        &mut budget(),
    )
    .unwrap();
    let (i, t) = job.next(source).unwrap().unwrap();
    job.reserve(i, &t, 100).unwrap();
    let review = json!({"edits":[],"missing":["Quạt chạy."],"unresolved":[]});
    job.finish(
        source,
        Ok(serde_json::to_vec(&review).unwrap()),
        1,
        "test",
        &mut budget(),
    )
    .unwrap();
    let (i, t) = job.next(source).unwrap().unwrap();
    assert_eq!(t.kind, TaskKind::Supplement);
    job.reserve(i, &t, 100).unwrap();
    let mut extra = draft();
    extra["statements"][0]["subject"] = json!("Quạt");
    extra["statements"][0]["predicate"] = json!("chạy");
    extra["statements"][0]["frequency"] = json!([]);
    extra["statements"][0]["evidence"] = json!("Quạt chạy.");
    job.finish(
        source,
        Ok(serde_json::to_vec(&extra).unwrap()),
        1,
        "test",
        &mut budget(),
    )
    .unwrap();
    let final_draft = &job.windows[0].revisions.last().unwrap().draft;
    assert_eq!(final_draft["statements"][0], draft()["statements"][0]);
    assert_eq!(final_draft["statements"].as_array().unwrap().len(), 2);
    assert!(validate(source, final_draft, &mut budget())
        .unwrap()
        .clear());
}
