use super::*;
fn budget() -> WorkBudget {
    WorkBudget::new(
        1_000_000,
        Duration::from_secs(30),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap()
}
const ROCKET: &str =
    "Tên lửa có thể sử dụng nhiên liệu lỏng hoặc rắn . Trong đó , nhiên liệu rắn sẽ an toàn hơn.";
fn rocket() -> Value {
    json!({"claims":[
        {"subject":"Tên lửa","predicate":"sử dụng","arguments":["nhiên liệu lỏng hoặc rắn"],"modality":["có thể"],"anchor":"Tên lửa có thể sử dụng nhiên liệu lỏng hoặc rắn",
        "alternatives":[{"quote":"nhiên liệu lỏng hoặc rắn","cue":"hoặc","exclusivity":"unspecified","branches":[{"surface":{"quote":"nhiên liệu lỏng"}},{"surface":{"quote":"rắn"},"borrowed":{"quote":"nhiên liệu"}}]}]},
        {"subject":"nhiên liệu rắn","predicate":"an toàn","time":["sẽ"],"anchor":"Trong đó , nhiên liệu rắn sẽ an toàn hơn",
        "references":[{"via":{"quote":"Trong đó"},"to":{"quote":"nhiên liệu lỏng hoặc rắn"},"target_kind":"group"}],
        "comparisons":[{"property":"an toàn","cue":"hơn","target_kind":"unspecified"}]}]})
}
#[test]
fn rocket_keeps_or_ellipsis_reference_and_unspecified_comparison() {
    let (d, v) = assemble(ROCKET, &rocket(), &mut budget()).unwrap();
    assert!(v.uncovered.is_empty(), "{:?}", v.uncovered);
    assert_eq!(
        v.issues,
        vec!["selection[1].comparisons: target_unspecified"]
    );
    assert_eq!(d["profile"], DRAFT_PROFILE);
    assert_eq!(
        d["statements"][0]["alternatives"][0]["branches"][1]["origin"],
        "reconstructed"
    );
    assert_eq!(
        d["statements"][1]["references"][0]["target_id"],
        "claim-0-group-0"
    );
    assert_eq!(d["statements"][1]["time"], json!(["sẽ"]));
    assert!(d["statements"][0]["numbers"].as_array().unwrap().is_empty());
    assert!(d["statements"][1]["relations"]
        .as_array()
        .unwrap()
        .is_empty());
}
#[test]
fn inferred_target_does_not_become_explicit_or_expand_local_evidence() {
    let mut r = rocket();
    r["claims"][1]["comparisons"][0]["target_kind"] = json!("implicit_candidate");
    r["claims"][1]["comparisons"][0]["target"] = json!({"quote":"nhiên liệu lỏng"});
    let (d, v) = assemble(ROCKET, &r, &mut budget()).unwrap();
    assert_eq!(d["statements"][1]["comparisons"][0]["origin"], "inferred");
    assert!(v.issues.iter().any(|x| x.contains("implicit_target")));
    assert!(!text(&d["statements"][1]["evidence"]).contains("lỏng"));
    r["claims"][1]["comparisons"][0]["target_kind"] = json!("explicit");
    assert!(!assemble(ROCKET, &r, &mut budget()).unwrap().1.clear());
}
#[test]
fn numeric_grammar_forbids_invented_numbers_on_long_and_short_sources() {
    for s in ["Vật an toàn.".to_string(), "Vật an toàn. ".repeat(40)] {
        let schema: Value = serde_json::from_str(&wire_schema(&s, None).unwrap()).unwrap();
        assert_eq!(
            schema["properties"]["claims"]["items"]["properties"]["quantities"]["maxItems"],
            0
        );
    }
    let r = json!({"claims":[{"subject":"Vật","predicate":"an toàn","quantities":[{"quote":"an toàn","unit":"toàn"}]}]});
    assert!(!assemble("Vật an toàn.", &r, &mut budget())
        .unwrap()
        .1
        .clear());
    let r = json!({"claims":[{"subject":"Hộp","predicate":"có","arguments":["bốn bánh"]}],"unresolved":[{"quote":"bốn bánh","reason":"unsupported_number_form"}]});
    assert!(!assemble("Hộp có bốn bánh.", &r, &mut budget())
        .unwrap()
        .1
        .clear());
}
#[test]
fn malformed_branch_and_ambiguous_reference_stay_visible() {
    let mut r = rocket();
    r["claims"][0]["alternatives"][0]["branches"][1]["surface"] = json!({"quote":"nhiên liệu rắn"});
    assert!(assemble(ROCKET, &r, &mut budget())
        .unwrap()
        .1
        .issues
        .iter()
        .any(|s| s.contains("branch_scope")));
    let r = json!({"claims":[{"subject":"Đèn","predicate":"sáng"},{"subject":"Đèn","predicate":"tắt"}]});
    let (_, v) = assemble("Đèn sáng. Đèn tắt.", &r, &mut budget()).unwrap();
    assert!(v.uncovered.iter().any(|s| s.contains("Đèn")));
}
#[test]
fn jobs_preserve_revision_and_reject_previous_executor_commitment() {
    let mut job = review_draft::DraftJob::new_selection_v2(ROCKET).unwrap();
    let (i, t) = job.next(ROCKET).unwrap().unwrap();
    assert_eq!(t.system_prompt(), PROMPT);
    job.reserve(i, &t, 100).unwrap();
    job.finish(
        ROCKET,
        Ok(serde_json::to_vec(&rocket()).unwrap()),
        10,
        "test",
        &mut budget(),
    )
    .unwrap();
    assert_eq!(job.state, "needs_review");
    let encoded = serde_json::to_value(&job).unwrap();
    let mut restored: review_draft::DraftJob = serde_json::from_value(encoded.clone()).unwrap();
    restored.interrupt();
    assert_eq!(serde_json::to_value(&restored).unwrap(), encoded);
    let mut old = review_draft::DraftJob::new_selection("Đèn sáng.").unwrap();
    old.commitment = "53cf766901d9e84b76902ea1d528d20618372cee9953174e76861db93d10f5c8".into();
    let old_bytes = serde_json::to_vec(&old).unwrap();
    assert!(old.next("Đèn sáng.").is_err());
    assert_eq!(serde_json::to_vec(&old).unwrap(), old_bytes);
}

#[test]
fn grammar_preserves_reviewed_order_and_repair_choices() {
    let wire = wire_schema("Máy dùng pin hoặc điện.", None).unwrap();
    assert!(wire.find("\"subject\"").unwrap() < wire.find("\"predicate\"").unwrap());
    let narrowed = repair_wire_schema(
        "Nước sôi ở 100oC",
        &json!(["arguments"]),
        &json!({"arguments":["100oC","ở 100oC"]}),
    )
    .unwrap();
    let value: Value = serde_json::from_str(&narrowed).unwrap();
    assert_eq!(
        value["properties"]["patch"]["properties"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        value["properties"]["patch"]["properties"]["arguments"]["items"]["enum"],
        json!(["100oC", "ở 100oC"])
    );
}

#[test]
fn missing_structure_and_clause_sized_qualifiers_are_not_mechanically_clear() {
    let choice =
        json!({"claims":[{"subject":"Máy","predicate":"dùng","arguments":["pin hoặc điện"]}]});
    assert!(assemble("Máy dùng pin hoặc điện.", &choice, &mut budget())
        .unwrap()
        .1
        .issues
        .iter()
        .any(|s| s.contains("unrepresented_choice")));
    let comparison = json!({"claims":[{"subject":"Xe đỏ","predicate":"nhanh hơn","time":["nhanh hơn xe xanh."]}]});
    assert!(
        !assemble("Xe đỏ nhanh hơn xe xanh.", &comparison, &mut budget())
            .unwrap()
            .1
            .clear()
    );
}

#[test]
fn selectors_cannot_escape_scope_and_atomic_patch_cannot_delete_a_branch() {
    let mut choice = rocket();
    choice["claims"][0]["alternatives"][0]["branches"][1]["surface"] =
        json!({"quote":"rắn","within":"nhiên liệu rắn sẽ an toàn hơn"});
    assert!(assemble(ROCKET, &choice, &mut budget())
        .unwrap()
        .1
        .issues
        .iter()
        .any(|s| s.contains("branch_scope")));
    let base = rocket();
    let mut groups = base["claims"][0]["alternatives"].clone();
    groups[0]["branches"]
        .as_array_mut()
        .unwrap()
        .push(json!({"surface":{"quote":"lỏng"}}));
    assert!(apply_repair(
        ROCKET,
        &base,
        0,
        &["alternatives".into()],
        &json!({"patch":{"alternatives":groups}}),
        &mut budget()
    )
    .is_err());
    let mut comp = base["claims"][1]["comparisons"].clone();
    comp[0]["target_kind"] = json!("explicit");
    comp[0]["target"] = json!({"quote":"nhiên liệu lỏng"});
    assert!(apply_repair(
        ROCKET,
        &base,
        1,
        &["comparisons".into()],
        &json!({"patch":{"comparisons":comp}}),
        &mut budget()
    )
    .is_err());
}

#[test]
fn numeric_substrings_and_resource_overflow_cannot_be_accepted() {
    let choice = json!({"claims":[{"subject":"Vật","predicate":"nặng","arguments":["1e3kg"],"quantities":[{"quote":"3kg","unit":"kg"}]}]});
    assert!(assemble("Vật nặng 1e3kg.", &choice, &mut budget())
        .unwrap()
        .1
        .issues
        .iter()
        .any(|s| s.contains("source_numeric_token")));
    let choice = json!({"claims":vec![json!({"subject":"Vật","predicate":"nặng","arguments":vec!["Vật";16]});16]});
    assert_eq!(
        assemble("Vật nặng.", &choice, &mut budget()).unwrap_err().0,
        "semantic_node_bound"
    );
    let mut canceled = WorkBudget::new(
        1_000_000,
        Duration::from_secs(30),
        Arc::new(AtomicBool::new(true)),
    )
    .unwrap();
    assert!(assemble(ROCKET, &rocket(), &mut canceled).is_err());
}

#[test]
fn explicit_comparison_and_unresolved_reference_remain_distinct() {
    let v = json!({"claims":[{"subject":"Xe đỏ","predicate":"nhanh","arguments":["xe xanh"],"comparisons":[{"property":"nhanh","cue":"hơn","target_kind":"explicit","target":{"quote":"xe xanh"}}]}]});
    assert!(assemble("Xe đỏ nhanh hơn xe xanh.", &v, &mut budget())
        .unwrap()
        .1
        .clear());
    let mut v = rocket();
    v["claims"][1]["references"][0]["to"] = json!({"quote":"không có"});
    assert!(assemble(ROCKET, &v, &mut budget())
        .unwrap()
        .1
        .issues
        .iter()
        .any(|s| s.contains("target_absent")));
}

#[test]
fn reference_cycles_and_cross_window_targets_remain_unresolved() {
    let source = "A chạy. B dừng.";
    let v = json!({"claims":[{"subject":"A","predicate":"chạy","references":[{"via":{"quote":"A"},"to":{"quote":"B dừng"},"target_kind":"claim"}]},{"subject":"B","predicate":"dừng","references":[{"via":{"quote":"B"},"to":{"quote":"A chạy"},"target_kind":"claim"}]}]});
    assert!(assemble(source, &v, &mut budget())
        .unwrap()
        .1
        .issues
        .iter()
        .any(|s| s.contains("cycle")));
    let v = json!({"claims":[{"subject":"B","predicate":"dừng","references":[{"via":{"quote":"B"},"to":{"quote":"A chạy"},"target_kind":"claim"}]}]});
    assert!(assemble("B dừng.", &v, &mut budget())
        .unwrap()
        .1
        .issues
        .iter()
        .any(|s| s.contains("target_absent")));
}

#[test]
fn borrowed_predicate_and_number_word_cannot_be_mechanically_clear() {
    let v = json!({"claims":[{"subject":"Máy","predicate":"dùng","arguments":["pin hoặc điện"],"alternatives":[{"quote":"pin hoặc điện","cue":"hoặc","branches":[{"surface":{"quote":"pin"}},{"surface":{"quote":"điện"},"borrowed":{"quote":"dùng"}}],"exclusivity":"unspecified"}]}]});
    assert!(!assemble("Máy dùng pin hoặc điện.", &v, &mut budget())
        .unwrap()
        .1
        .clear());
    let v = json!({"claims":[{"subject":"Xe","predicate":"có","arguments":["bốn bánh"]}]});
    assert!(assemble("Xe có bốn bánh.", &v, &mut budget())
        .unwrap()
        .1
        .issues
        .iter()
        .any(|s| s.contains("number_word")));
}

#[test]
fn conjunction_cannot_be_mechanically_accepted_as_or() {
    let v = json!({"claims":[{"subject":"Hộp","predicate":"chứa","arguments":["bi xanh và bi đỏ"],"alternatives":[{"quote":"bi xanh và bi đỏ","cue":"và","branches":[{"surface":{"quote":"bi xanh"}},{"surface":{"quote":"bi đỏ"}}],"exclusivity":"unspecified"}]}]});
    assert!(assemble("Hộp chứa bi xanh và bi đỏ.", &v, &mut budget())
        .unwrap()
        .1
        .issues
        .iter()
        .any(|s| s.contains("incompatible_or_cue")));
}

#[test]
fn prior_simple_roles_remain_representable_without_semantic_extensions() {
    let cases = [
        (
            "Nước thường sôi ở 100oC ở gần mặt nước biển",
            json!({"claims":[{"subject":"Nước","predicate":"sôi","arguments":["ở 100oC"],"frequency":["thường"],"location":["ở gần mặt nước biển"],"quantities":[{"quote":"100oC","unit":"oC"}]}]}),
        ),
        (
            "Xe oto cá nhân thường có 4 bánh",
            json!({"claims":[{"subject":"Xe oto cá nhân","predicate":"có","arguments":["4 bánh"],"frequency":["thường"],"quantities":[{"quote":"4 bánh","counted_entity":"bánh"}]}]}),
        ),
        (
            "Nếu trời mưa, đường trơn.",
            json!({"claims":[{"subject":"đường","predicate":"trơn","condition":["Nếu trời mưa"]}]}),
        ),
        (
            "Cảm biến ghi 12kg. Đèn không sáng, nhưng quạt chạy.",
            json!({"claims":[{"subject":"Cảm biến","predicate":"ghi","arguments":["12kg"],"quantities":[{"quote":"12kg","unit":"kg"}]},{"subject":"Đèn","predicate":"sáng","negation":["không"]},{"subject":"quạt","predicate":"chạy","links":[{"to":"Đèn không sáng","kind":"contrast","via":"nhưng"}]}]}),
        ),
    ];
    for (source, selection) in cases {
        let (_, validation) = assemble(source, &selection, &mut budget()).unwrap();
        assert!(
            validation.clear(),
            "{:?} {:?}",
            validation.issues,
            validation.uncovered
        );
    }
}

#[test]
fn whole_job_node_budget_counts_retained_windows() {
    let source = "Đèn sáng. ".repeat(200);
    let mut job = review_draft::DraftJob::new_selection_v2(&source).unwrap();
    let choice = json!({"claims":[{"subject":"Đèn","predicate":"sáng"}]});
    let (mut draft, validation) = assemble("Đèn sáng.", &choice, &mut budget()).unwrap();
    draft["semantic_nodes"] = json!(256);
    job.windows[0].revisions.push(review_draft::Revision {
        draft: draft.clone(),
        producer: "test".into(),
        task: review_draft::TaskKind::Select,
        validation,
    });
    job.windows[0].state = "needs_review".into();
    let (i, t) = job.next(&source).unwrap().unwrap();
    job.reserve(i, &t, 100).unwrap();
    job.finish(
        &source,
        Ok(serde_json::to_vec(&choice).unwrap()),
        1,
        "test",
        &mut budget(),
    )
    .unwrap();
    assert!(job.issues.contains(&"semantic_node_bound".into()));
    assert!(job.windows[i].revisions.is_empty());
    assert_eq!(job.windows[0].revisions[0].draft, draft);
}

#[test]
fn qualifier_overlap_dispatches_only_source_local_predicate_repair() {
    let source = "Máy có thể chạy.";
    let selection =
        json!({"claims":[{"subject":"Máy","predicate":"có thể chạy","modality":["có thể"]}]});
    let (_, validation) = assemble(source, &selection, &mut budget()).unwrap();
    let (target, fields) = repair_target(source, &selection, &validation, &mut budget())
        .unwrap()
        .unwrap();
    assert_eq!(target, 0);
    assert_eq!(fields, vec!["predicate"]);
    let choices = repair_choices(
        source,
        &selection,
        target,
        &fields,
        &validation,
        &mut budget(),
    )
    .unwrap();
    let wire: Value =
        serde_json::from_str(&repair_wire_schema(source, &json!(fields), &choices).unwrap())
            .unwrap();
    assert_eq!(
        wire["properties"]["patch"]["properties"]["predicate"]["enum"],
        json!(["chạy", "có thể chạy"])
    );
    let (changed, _, after) = apply_repair(
        source,
        &selection,
        target,
        &fields,
        &json!({"patch":{"predicate":"chạy"}}),
        &mut budget(),
    )
    .unwrap();
    assert!(after.clear(), "{:?}", after.issues);
    assert_eq!(
        changed["claims"][0]["modality"],
        selection["claims"][0]["modality"]
    );
    assert!(apply_repair(
        source,
        &selection,
        target,
        &fields,
        &json!({"patch":{"predicate":"Máy"}}),
        &mut budget()
    )
    .is_err());
    assert!(apply_repair(
        source,
        &selection,
        target,
        &fields,
        &json!({"patch":{"predicate":"có thể chạy"}}),
        &mut budget()
    )
    .is_err());
    assert_eq!(
        predicate_choices(&json!({"predicate":"cannot run","negation":["can"]})),
        BTreeSet::from(["cannot run".into()])
    );
    assert_eq!(
        predicate_choices(&json!({"predicate":"go often home","frequency":["often"]})),
        BTreeSet::from(["go often home".into()])
    );
}
