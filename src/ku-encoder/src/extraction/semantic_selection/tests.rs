use super::*;
fn budget() -> WorkBudget {
    WorkBudget::new(
        1_000_000,
        Duration::from_secs(30),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap()
}
#[test]
fn selection_host_builds_defaults_numbers_and_source_without_model_bookkeeping() {
    let source = "Xe oto cá nhân thường có 4 bánh";
    let selection = json!({"claims":[{"subject":"Xe oto cá nhân","predicate":"có","arguments":["4 bánh"],"frequency":["thường"],"quantities":[{"quote":"4 bánh","counted_entity":"bánh"}]}]});
    let (draft, v) = assemble(source, &selection, &mut budget()).unwrap();
    assert!(v.clear());
    let c = &draft["statements"][0];
    assert_eq!(
        c["numbers"],
        json!([{"value_quote":"4","unit_quote":"","counted_entity_quote":"bánh"}])
    );
    assert_eq!(c["evidence"], source);
    assert_eq!(c["negation"], json!([]));
    assert_eq!(c["relations"], json!([]));
    assert!(selection["claims"][0].get("evidence").is_none());
}
#[test]
fn selection_link_resolution_uses_quoted_target_not_first_claim() {
    let source = "Cảm biến ghi 12kg. Đèn không sáng, nhưng quạt chạy.";
    let mut selection = json!({"claims":[{"subject":"Cảm biến","predicate":"ghi","arguments":["12kg"],"quantities":[{"quote":"12kg","unit":"kg"}]},{"subject":"Đèn","predicate":"sáng","negation":["không"]},{"subject":"quạt","predicate":"chạy","links":[{"to":"Đèn không sáng","kind":"contrast","via":"nhưng"}]}]});
    let (draft, v) = assemble(source, &selection, &mut budget()).unwrap();
    assert!(v.clear());
    assert_eq!(draft["statements"][2]["relations"][0]["statement"], 1);
    selection["claims"][2]["links"][0]["to"] = json!("quạt chạy");
    assert!(assemble(source, &selection, &mut budget())
        .unwrap()
        .1
        .issues
        .iter()
        .any(|i| i.contains("self_target")));
    selection["claims"][2]["links"][0]["to"] = json!("Cảm biến ghi 12kg. Đèn không sáng");
    assert!(assemble(source, &selection, &mut budget())
        .unwrap()
        .1
        .issues
        .iter()
        .any(|i| i.contains("ambiguous")));
    selection["claims"][2]["links"][0]["to"] = json!("Không có trong nguồn");
    assert!(!assemble(source, &selection, &mut budget())
        .unwrap()
        .1
        .clear());
}
#[test]
fn selection_repeated_quotes_need_disambiguation_and_fabrication_is_visible() {
    let source = "Đèn sáng. Đèn tắt.";
    let mut selection = json!({"claims":[{"subject":"Đèn","predicate":"sáng"},{"subject":"Đèn","predicate":"tắt"}]});
    assert!(!assemble(source, &selection, &mut budget())
        .unwrap()
        .1
        .clear());
    selection["claims"][0]["anchor"] = json!("Đèn sáng");
    selection["claims"][1]["anchor"] = json!("Đèn tắt");
    assert!(assemble(source, &selection, &mut budget())
        .unwrap()
        .1
        .clear());
    selection["claims"][0]["subject"] = json!("Máy");
    assert!(!assemble(source, &selection, &mut budget())
        .unwrap()
        .1
        .clear());
}
#[test]
fn selection_targeted_repair_cannot_change_other_fields_or_drop_source() {
    let source = "Nước sôi ở 100oC";
    let selection = json!({"claims":[{"subject":"Nước","predicate":"sôi","arguments":["100oC"],"quantities":[{"quote":"100oC","unit":"oC"}]}]});
    let (_, v) = assemble(source, &selection, &mut budget()).unwrap();
    let (i, fields) = repair_target(source, &selection, &v, &mut budget())
        .unwrap()
        .unwrap();
    assert_eq!(fields, vec!["arguments"]);
    let reply = json!({"patch":{"arguments":["ở 100oC"]}});
    assert!(
        apply_repair(source, &selection, i, &fields, &reply, &mut budget())
            .unwrap()
            .2
            .clear()
    );
    assert_eq!(
        apply_repair(
            source,
            &selection,
            i,
            &fields,
            &json!({"patch":{"subject":"100oC"}}),
            &mut budget()
        )
        .unwrap_err()
        .0,
        "selection_repair_scope"
    );
    assert!(apply_repair(
        source,
        &selection,
        i,
        &fields,
        &json!({"patch":{"arguments":[]}}),
        &mut budget()
    )
    .is_err());
    assert!(apply_repair(
        source,
        &selection,
        i,
        &fields,
        &json!({"patch":{"arguments":["ở 101oC"]}}),
        &mut budget()
    )
    .is_err());
    assert_eq!(
        apply_repair(
            source,
            &selection,
            i,
            &fields,
            &json!({"patch":{"arguments":[source]}}),
            &mut budget()
        )
        .unwrap_err()
        .0,
        "selection_repair_choice"
    );
}

#[test]
fn selection_repair_choices_exclude_self_and_remain_narrow_in_wire_schema() {
    let source = "Pin yếu nhưng máy chạy.";
    let selection = json!({"claims":[{"subject":"Pin","predicate":"yếu","links":[{"to":"Pin yếu","kind":"contrast","via":"nhưng"}]},{"subject":"máy","predicate":"chạy"}]});
    let (_, v) = assemble(source, &selection, &mut budget()).unwrap();
    let fields = vec!["links".to_owned()];
    let choices = repair_choices(source, &selection, 0, &fields, &v, &mut budget()).unwrap();
    assert_eq!(choices["to"], json!(["máy chạy"]));
    for text in [
        source.to_string(),
        format!("{} {}", source, "ngữ cảnh ".repeat(50)),
    ] {
        let request = review_draft::TaskRequest {
            kind: review_draft::TaskKind::RepairSelection,
            data: json!({"SOURCE":text,"FIELDS":fields,"CHOICES":choices}),
            deadline: Duration::from_secs(30),
        };
        let wire: Value = serde_json::from_str(&request.wire_schema().unwrap()).unwrap();
        assert_eq!(
            wire["properties"]["patch"]["properties"]["links"]["items"]["properties"]["to"]["enum"],
            json!(["máy chạy"])
        );
    }
    let repaired = apply_repair(
        source,
        &selection,
        0,
        &fields,
        &json!({"patch":{"links":[{"to":"máy chạy","kind":"contrast","via":"nhưng"}]}}),
        &mut budget(),
    )
    .unwrap();
    assert!(repaired.2.clear());
    assert_eq!(repaired.1["statements"][0]["relations"][0]["statement"], 1);
    let argument_request = review_draft::TaskRequest {
        kind: review_draft::TaskKind::RepairSelection,
        data: json!({"SOURCE":"Vật nặng ở 12kg", "FIELDS":["arguments"], "CHOICES":{"arguments":["12kg","ở 12kg"]}}),
        deadline: Duration::from_secs(30),
    };
    let wire: Value = serde_json::from_str(&argument_request.wire_schema().unwrap()).unwrap();
    assert_eq!(
        wire["properties"]["patch"]["properties"]["arguments"]["items"]["enum"],
        json!(["12kg", "ở 12kg"])
    );
}
#[test]
fn selection_success_is_unassessed_one_call_and_survives_read_after_restart() {
    let source = "Đèn sáng.";
    let mut job = review_draft::DraftJob::new_selection(source).unwrap();
    let (i, task) = job.next(source).unwrap().unwrap();
    assert_eq!(task.kind, review_draft::TaskKind::Select);
    job.reserve(i, &task, 100).unwrap();
    job.finish(
        source,
        Ok(serde_json::to_vec(&json!({"claims":[{"subject":"Đèn","predicate":"sáng"}]})).unwrap()),
        1,
        "test",
        &mut budget(),
    )
    .unwrap();
    assert_eq!(job.state, "draft_extracted");
    assert_eq!(job.calls, 1);
    assert!(job.next(source).unwrap().is_none());
    let mut restored: review_draft::DraftJob =
        serde_json::from_value(serde_json::to_value(job).unwrap()).unwrap();
    restored.interrupt();
    assert_eq!(restored.state, "draft_extracted");
    assert!(restored.next(source).unwrap().is_none());
}
#[test]
fn selection_failed_repair_keeps_prior_draft_and_recorded_proposal() {
    let source = "Nước sôi ở 100oC";
    let mut job = review_draft::DraftJob::new_selection(source).unwrap();
    let (i, t) = job.next(source).unwrap().unwrap();
    job.reserve(i, &t, 100).unwrap();
    job.finish(source,Ok(serde_json::to_vec(&json!({"claims":[{"subject":"Nước","predicate":"sôi","arguments":["100oC"],"quantities":[{"quote":"100oC","unit":"oC"}]}]})).unwrap()),1,"test",&mut budget()).unwrap();
    assert_eq!(job.state, "queued");
    let before = job.windows[0].revisions[0].draft.clone();
    let (i, t) = job.next(source).unwrap().unwrap();
    assert_eq!(t.data["FIELDS"], json!(["arguments"]));
    assert!(t.data.get("DRAFT").is_none());
    job.reserve(i, &t, 100).unwrap();
    job.finish(
        source,
        Ok(r#"{"patch":{"subject":"sôi"}}"#.as_bytes().to_vec()),
        1,
        "test",
        &mut budget(),
    )
    .unwrap();
    assert_eq!(job.state, "needs_review");
    assert_eq!(job.windows[0].selection_proposals.len(), 2);
    assert_eq!(job.windows[0].revisions.len(), 1);
    assert_eq!(job.windows[0].revisions[0].draft, before);
}

#[test]
fn selection_limits_unknown_fields_and_cancellation_remain_host_owned() {
    let bad = json!({"claims":[{"subject":"Đèn","predicate":"sáng","verified":true}]});
    assert!(assemble("Đèn sáng.", &bad, &mut budget()).is_err());
    let excessive = json!({"claims":vec![json!({"subject":"Đèn","predicate":"sáng"});65]});
    assert!(assemble("Đèn sáng.", &excessive, &mut budget()).is_err());
    let mut canceled = WorkBudget::new(
        1_000_000,
        Duration::from_secs(30),
        Arc::new(AtomicBool::new(true)),
    )
    .unwrap();
    assert!(numeric_candidates("Đèn 12W.", &mut canceled).is_err());
}

#[test]
fn selection_claim_bound_counts_prior_windows_and_preserves_their_revision() {
    let source = "Đèn sáng. ".repeat(200);
    let mut job = review_draft::DraftJob::new_selection(&source).unwrap();
    assert!(job.windows.len() > 1);
    let choice = json!({"claims":[{"subject":"Đèn","predicate":"sáng"}]});
    let (mut prior, validation) = assemble("Đèn sáng.", &choice, &mut budget()).unwrap();
    prior["statements"] = json!(vec![prior["statements"][0].clone(); 64]);
    job.windows[0].revisions.push(review_draft::Revision {
        draft: prior.clone(),
        producer: "test".into(),
        task: review_draft::TaskKind::Select,
        validation,
    });
    job.windows[0].state = "needs_review".into();
    let (i, request) = job.next(&source).unwrap().unwrap();
    assert_eq!(i, 1);
    job.reserve(i, &request, 100).unwrap();
    job.finish(
        &source,
        Ok(serde_json::to_vec(&choice).unwrap()),
        1,
        "test",
        &mut budget(),
    )
    .unwrap();
    assert!(job.issues.contains(&"claim_bound".into()));
    assert!(job.windows[1].revisions.is_empty());
    assert_eq!(job.windows[0].revisions[0].draft, prior);
}

#[test]
fn selection_inert_structural_default_is_not_a_successful_repair() {
    let source = "Đèn sáng.";
    let base = json!({"claims":[{"subject":"Đèn","predicate":"sáng"}]});
    let result = apply_repair(
        source,
        &base,
        0,
        &["frequency".into()],
        &json!({"patch":{"frequency":[]}}),
        &mut budget(),
    );
    assert_eq!(result.unwrap_err().0, "selection_repair_no_change");
}

#[test]
fn selection_repair_decoder_exposes_only_declared_fields_on_short_and_long_sources() {
    for source in ["Đèn sáng.".to_string(), "Đèn sáng. ".repeat(30)] {
        let t = review_draft::TaskRequest {
            kind: review_draft::TaskKind::RepairSelection,
            data: json!({"SOURCE":source,"FIELDS":["arguments"]}),
            deadline: Duration::from_secs(30),
        };
        let wire: Value = serde_json::from_str(&t.wire_schema().unwrap()).unwrap();
        let fields = wire["properties"]["patch"]["properties"]
            .as_object()
            .unwrap();
        assert_eq!(fields.len(), 1);
        assert!(fields.contains_key("arguments"));
        assert_eq!(wire["properties"]["patch"]["additionalProperties"], false);
    }
    assert!(repair_wire_schema(&json!(["publish"]), &json!({})).is_err());
}

#[test]
fn selection_cue_rules_only_route_a_missing_qualifier_without_assigning_meaning() {
    let source = "Pin đôi khi nóng.";
    let base = json!({"claims":[{"subject":"Pin","predicate":"nóng"}]});
    let (draft, validation) = assemble(source, &base, &mut budget()).unwrap();
    assert_eq!(draft["statements"][0]["frequency"], json!([]));
    assert_eq!(
        repair_target(source, &base, &validation, &mut budget()).unwrap(),
        Some((0, vec!["frequency".into()]))
    );
    let patch = json!({"patch":{"frequency":["đôi khi"]}});
    assert!(apply_repair(
        source,
        &base,
        0,
        &["frequency".into()],
        &patch,
        &mut budget()
    )
    .unwrap()
    .2
    .clear());
    let unrelated = json!({"claims":[{"subject":"Pin","predicate":"nóng"}]});
    let (_, v) = assemble("Pin nóng. Máy đôi khi dừng.", &unrelated, &mut budget()).unwrap();
    assert!(
        repair_target("Pin nóng. Máy đôi khi dừng.", &unrelated, &v, &mut budget())
            .unwrap()
            .is_none()
    );
}

#[test]
fn selection_host_does_not_split_unsupported_number_syntax_into_valid_fragments() {
    for quantity in ["1e3kg", "1,000kg", ".5kg", "+1kg"] {
        let source = format!("Vật nặng {quantity}.");
        let base = json!({"claims":[{"subject":"Vật","predicate":"nặng","arguments":[quantity],"quantities":[{"quote":quantity,"unit":"kg"}]}]});
        let (draft, v) = assemble(&source, &base, &mut budget()).unwrap();
        assert_eq!(
            draft["statements"][0]["numbers"].as_array().unwrap().len(),
            1
        );
        assert!(
            v.issues.iter().any(|s| s.contains("unsupported_number")),
            "{quantity}: {:?}",
            v.issues
        );
    }
}

#[test]
fn selection_quantity_repair_cannot_discard_a_prior_number() {
    let source = "Hộp chứa 5 viên bi và 2 con chim.";
    let base = json!({"claims":[{"subject":"Hộp","predicate":"chứa","arguments":["5 viên bi và 2 con chim"],"quantities":[{"quote":"5 viên bi","counted_entity":"viên bi"},{"quote":"2 con chim","counted_entity":"con chim"}]}]});
    let patch = json!({"patch":{"quantities":[{"quote":"5 viên bi","counted_entity":"viên bi"}]}});
    assert!(apply_repair(
        source,
        &base,
        0,
        &["quantities".into()],
        &patch,
        &mut budget()
    )
    .is_err());
    let mut malformed = base;
    malformed["claims"][0]["quantities"][0]["quote"] = json!("5 viên bi và 2 con chim");
    assert!(assemble(source, &malformed, &mut budget())
        .unwrap()
        .1
        .issues
        .iter()
        .any(|s| s.contains("number_or_annotation_scope")));
}

#[test]
fn selection_call_binding_rejects_stale_repair_scope_and_keeps_checkpoint() {
    let source = "Nước sôi ở 100oC";
    let mut job = review_draft::DraftJob::new_selection(source).unwrap();
    let (i, t) = job.next(source).unwrap().unwrap();
    job.reserve(i, &t, 100).unwrap();
    job.finish(source,Ok(serde_json::to_vec(&json!({"claims":[{"subject":"Nước","predicate":"sôi","arguments":["100oC"],"quantities":[{"quote":"100oC","unit":"oC"}]}]})).unwrap()),1,"test",&mut budget()).unwrap();
    let (i, t) = job.next(source).unwrap().unwrap();
    job.reserve(i, &t, 100).unwrap();
    job.active.as_mut().unwrap().repair_scope.as_mut().unwrap()["TARGET"]["subject"] = json!("sôi");
    job.finish(
        source,
        Ok(serde_json::to_vec(&json!({"patch":{"arguments":["ở 100oC"]}})).unwrap()),
        1,
        "test",
        &mut budget(),
    )
    .unwrap();
    assert_eq!(job.state, "needs_review");
    assert_eq!(job.windows[0].revisions.len(), 1);
    assert!(job.issues.contains(&"selection_repair_binding".into()));
}
