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

#[test]
fn borrowed_outside_other_branch_dispatches_only_source_local_branch_repair() {
    // D-060: "dùng" is borrowed into branch "điện" from outside the group.
    let source = "Máy dùng pin hoặc điện.";
    let selection = json!({"claims":[{"subject":"Máy","predicate":"dùng","arguments":["pin hoặc điện"],"alternatives":[{"quote":"pin hoặc điện","cue":"hoặc","exclusivity":"unspecified","branches":[{"surface":{"quote":"pin"}},{"surface":{"quote":"điện"},"borrowed":{"quote":"dùng"}}]}]}]});
    let (_, validation) = assemble(source, &selection, &mut budget()).unwrap();
    assert!(validation
        .issues
        .contains(&"selection[0].alternatives: borrowed_ambiguous".into()));
    let (target, fields) = repair_target(source, &selection, &validation, &mut budget())
        .unwrap()
        .unwrap();
    assert_eq!((target, fields.clone()), (0, vec!["alternatives".into()]));
    let choices = repair_choices(
        source,
        &selection,
        target,
        &fields,
        &validation,
        &mut budget(),
    )
    .unwrap();
    assert_eq!(
        choices["alternatives"],
        json!({"groups":1,"repairable":[{"group":0,"branch":1,"borrowed":["pin"],"may_omit":true}],"borrowed":["pin"]})
    );
    let wire: Value =
        serde_json::from_str(&repair_wire_schema(source, &json!(fields), &choices).unwrap())
            .unwrap();
    let alt = &wire["properties"]["patch"]["properties"]["alternatives"];
    assert_eq!(
        (alt["minItems"].clone(), alt["maxItems"].clone()),
        (json!(1), json!(1))
    );
    assert_eq!(
        alt["items"]["properties"]["branches"]["items"]["properties"]["borrowed"]["properties"]
            ["quote"]["enum"],
        json!(["pin"])
    );
    // Accepted: drop the reconstruction; the branch keeps its exact surface.
    let mut dropped = selection["claims"][0]["alternatives"].clone();
    dropped[0]["branches"][1]
        .as_object_mut()
        .unwrap()
        .remove("borrowed");
    let (changed, draft, after) = apply_repair(
        source,
        &selection,
        target,
        &fields,
        &json!({"patch":{"alternatives":dropped}}),
        &mut budget(),
    )
    .unwrap();
    assert!(after.clear(), "{:?} {:?}", after.issues, after.uncovered);
    assert_eq!(
        draft["statements"][0]["alternatives"][0]["branches"][1]["origin"],
        "explicit"
    );
    assert_eq!(changed["claims"][0]["predicate"], "dùng");
    // Rejected: unchanged, out-of-choice, extra selector, changed surface/cue, extra group.
    let reject = |alternatives: Value| {
        apply_repair(
            source,
            &selection,
            target,
            &fields,
            &json!({"patch":{"alternatives":alternatives}}),
            &mut budget(),
        )
        .is_err()
    };
    assert!(reject(selection["claims"][0]["alternatives"].clone()));
    let mut out_of_choice = selection["claims"][0]["alternatives"].clone();
    out_of_choice[0]["branches"][1]["borrowed"] = json!({"quote":"Máy"});
    assert!(reject(out_of_choice));
    let mut with_selector = selection["claims"][0]["alternatives"].clone();
    with_selector[0]["branches"][1]["borrowed"] = json!({"quote":"pin","within":"pin hoặc điện"});
    assert!(reject(with_selector));
    let mut surface = dropped.clone();
    surface[0]["branches"][1]["surface"] = json!({"quote":"điện","within":"pin hoặc điện"});
    assert!(reject(surface));
    let mut cue = dropped.clone();
    cue[0]["cue"] = json!("và");
    assert!(reject(cue));
    let mut extra = dropped.clone();
    extra.as_array_mut().unwrap().push(dropped[0].clone());
    assert!(reject(extra));
}

#[test]
fn borrowed_branch_repair_keeps_valid_reconstructions_and_flags_only_unsourced_text() {
    // A valid reconstruction is never dispatched or editable.
    let base = rocket();
    let (_, validation) = assemble(ROCKET, &base, &mut budget()).unwrap();
    assert!(repair_target(ROCKET, &base, &validation, &mut budget())
        .unwrap()
        .is_none());
    let mut renamed = base["claims"][0]["alternatives"].clone();
    renamed[0]["branches"][1]["borrowed"] = json!({"quote":"liệu"});
    assert!(apply_repair(
        ROCKET,
        &base,
        0,
        &["alternatives".into()],
        &json!({"patch":{"alternatives":renamed}}),
        &mut budget()
    )
    .is_err());
    // Borrowed cue text inside the group: choices are token runs of the other branch.
    let mut cue_borrowed = rocket();
    cue_borrowed["claims"][0]["alternatives"][0]["branches"][1]["borrowed"] =
        json!({"quote":"hoặc"});
    let (_, validation) = assemble(ROCKET, &cue_borrowed, &mut budget()).unwrap();
    assert!(validation
        .issues
        .contains(&"selection[0].alternatives: borrowed_outside_other_branch".into()));
    let (target, fields) = repair_target(ROCKET, &cue_borrowed, &validation, &mut budget())
        .unwrap()
        .unwrap();
    assert_eq!((target, fields.clone()), (0, vec!["alternatives".into()]));
    let choices = repair_choices(
        ROCKET,
        &cue_borrowed,
        target,
        &fields,
        &validation,
        &mut budget(),
    )
    .unwrap();
    assert_eq!(
        choices["alternatives"]["repairable"][0]["borrowed"],
        json!([
            "liệu",
            "liệu lỏng",
            "lỏng",
            "nhiên",
            "nhiên liệu",
            "nhiên liệu lỏng"
        ])
    );
    let (changed, _, after) = apply_repair(
        ROCKET,
        &cue_borrowed,
        target,
        &fields,
        &json!({"patch":{"alternatives":base["claims"][0]["alternatives"]}}),
        &mut budget(),
    )
    .unwrap();
    assert_eq!(changed, base);
    assert_eq!(
        after.issues,
        vec!["selection[1].comparisons: target_unspecified"]
    );
    // Text outside the group (`borrowed_ambiguous`) gets the same bounded choices.
    let mut outside = rocket();
    outside["claims"][0]["alternatives"][0]["branches"][1]["borrowed"] = json!({"quote":"Tên lửa"});
    let (_, validation) = assemble(ROCKET, &outside, &mut budget()).unwrap();
    assert!(validation
        .issues
        .contains(&"selection[0].alternatives: borrowed_ambiguous".into()));
    assert_eq!(
        repair_target(ROCKET, &outside, &validation, &mut budget()).unwrap(),
        Some((0, vec!["alternatives".into()]))
    );
    assert_eq!(
        borrowed_choices(ROCKET, &outside["claims"][0], &mut budget()).unwrap()[&(0, 1)].len(),
        6
    );
    // Unflagged reconstructions stay byte-identical inside a dispatched repair.
    let mut both = cue_borrowed["claims"][0]["alternatives"].clone();
    both[0]["branches"][0]["borrowed"] = json!({"quote":"rắn"});
    both[0]["branches"][1]
        .as_object_mut()
        .unwrap()
        .remove("borrowed");
    assert!(apply_repair(
        ROCKET,
        &cue_borrowed,
        target,
        &fields,
        &json!({"patch":{"alternatives":both}}),
        &mut budget()
    )
    .is_err());
    // No branch is flagged when there is no borrowed text at all.
    let mut plain = rocket();
    plain["claims"][0]["alternatives"][0]["branches"][1]
        .as_object_mut()
        .unwrap()
        .remove("borrowed");
    assert!(borrowed_choices(ROCKET, &plain["claims"][0], &mut budget())
        .unwrap()
        .is_empty());
}

#[test]
fn branch_scope_or_order_dispatches_bounded_reorder_and_surface_narrowing() {
    // D-061 Part 1: disjoint resolved branches emitted out of source byte order.
    let expected = rocket();
    let mut reversed = rocket();
    reversed["claims"][0]["alternatives"][0]["branches"] = json!([
        {"surface":{"quote":"rắn"},"borrowed":{"quote":"nhiên liệu"}},
        {"surface":{"quote":"nhiên liệu lỏng"}}
    ]);
    let (_, validation) = assemble(ROCKET, &reversed, &mut budget()).unwrap();
    assert!(validation
        .issues
        .contains(&"selection[0].alternatives: branch_scope_or_order".into()));
    let (target, fields) = repair_target(ROCKET, &reversed, &validation, &mut budget())
        .unwrap()
        .unwrap();
    assert_eq!((target, fields.clone()), (0, vec!["alternatives".into()]));
    let choices = repair_choices(
        ROCKET,
        &reversed,
        target,
        &fields,
        &validation,
        &mut budget(),
    )
    .unwrap();
    assert_eq!(
        choices["alternatives"]["scope_or_order"],
        json!([{"group":0,"reorder":true,"branches":[]}])
    );
    assert_eq!(
        choices["alternatives"]["surfaces"],
        json!(["nhiên liệu lỏng", "rắn"])
    );
    assert_eq!(choices["alternatives"]["borrowed"], json!(["nhiên liệu"]));
    let wire: Value =
        serde_json::from_str(&repair_wire_schema(ROCKET, &json!(fields), &choices).unwrap())
            .unwrap();
    let branch_props = &wire["properties"]["patch"]["properties"]["alternatives"]["items"]
        ["properties"]["branches"]["items"]["properties"];
    assert_eq!(
        branch_props["surface"]["properties"]["quote"]["enum"],
        json!(["nhiên liệu lỏng", "rắn"])
    );
    assert_eq!(
        branch_props["borrowed"]["properties"]["quote"]["enum"],
        json!(["nhiên liệu"])
    );
    let (changed, draft, after) = apply_repair(
        ROCKET,
        &reversed,
        target,
        &fields,
        &json!({"patch":{"alternatives":expected["claims"][0]["alternatives"]}}),
        &mut budget(),
    )
    .unwrap();
    assert_eq!(changed, expected);
    assert!(after.uncovered.is_empty(), "{:?}", after.uncovered);
    assert_eq!(
        after.issues,
        vec!["selection[1].comparisons: target_unspecified"]
    );
    assert_eq!(
        draft["statements"][0]["alternatives"][0]["branches"][1]["origin"],
        "reconstructed"
    );
    // Rejections on pure reorder: unchanged, dropped borrowed, moved borrowed, narrowed valid surface.
    let reject_rev = |alternatives: Value| {
        apply_repair(
            ROCKET,
            &reversed,
            target,
            &fields,
            &json!({"patch":{"alternatives":alternatives}}),
            &mut budget(),
        )
        .is_err()
    };
    assert!(reject_rev(reversed["claims"][0]["alternatives"].clone()));
    let mut dropped_borrowed = expected["claims"][0]["alternatives"].clone();
    dropped_borrowed[0]["branches"][1]
        .as_object_mut()
        .unwrap()
        .remove("borrowed");
    assert!(reject_rev(dropped_borrowed));
    let mut moved_borrowed = expected["claims"][0]["alternatives"].clone();
    moved_borrowed[0]["branches"][0]["borrowed"] = json!({"quote":"nhiên liệu"});
    moved_borrowed[0]["branches"][1]
        .as_object_mut()
        .unwrap()
        .remove("borrowed");
    assert!(reject_rev(moved_borrowed));
    let mut narrowed_valid = expected["claims"][0]["alternatives"].clone();
    narrowed_valid[0]["branches"][0]["surface"] = json!({"quote":"lỏng"});
    assert!(reject_rev(narrowed_valid));

    // D-061 Part 2: expanded or scope-escaping branch surface narrows inside group.quote outside cue.
    for bad_surface in [
        json!({"quote":"nhiên liệu rắn"}),
        json!({"quote":"rắn","within":"nhiên liệu rắn sẽ an toàn hơn"}),
    ] {
        let mut expanded = rocket();
        expanded["claims"][0]["alternatives"][0]["branches"][1]["surface"] = bad_surface;
        let (_, v_exp) = assemble(ROCKET, &expanded, &mut budget()).unwrap();
        assert!(v_exp
            .issues
            .contains(&"selection[0].alternatives: branch_scope_or_order".into()));
        let (t, f) = repair_target(ROCKET, &expanded, &v_exp, &mut budget())
            .unwrap()
            .unwrap();
        let ch = repair_choices(ROCKET, &expanded, t, &f, &v_exp, &mut budget()).unwrap();
        assert_eq!(
            ch["alternatives"]["scope_or_order"],
            json!([{"group":0,"reorder":true,"branches":[{"branch":1,"surface":["rắn"],"may_keep":false}]}])
        );
        assert_eq!(
            ch["alternatives"]["surfaces"],
            json!(["nhiên liệu lỏng", "rắn"])
        );
        let (repaired, _, after_exp) = apply_repair(
            ROCKET,
            &expanded,
            t,
            &f,
            &json!({"patch":{"alternatives":expected["claims"][0]["alternatives"]}}),
            &mut budget(),
        )
        .unwrap();
        assert_eq!(repaired, expected);
        assert!(after_exp.uncovered.is_empty());
        // Reject extra `within` on narrowed surface or cue-overlapping surface.
        let mut with_within = expected["claims"][0]["alternatives"].clone();
        with_within[0]["branches"][1]["surface"] =
            json!({"quote":"rắn","within":"nhiên liệu lỏng hoặc rắn"});
        assert!(apply_repair(
            ROCKET,
            &expanded,
            t,
            &f,
            &json!({"patch":{"alternatives":with_within}}),
            &mut budget(),
        )
        .is_err());
        let mut with_cue = expected["claims"][0]["alternatives"].clone();
        with_cue[0]["branches"][1]["surface"] = json!({"quote":"hoặc rắn"});
        assert!(apply_repair(
            ROCKET,
            &expanded,
            t,
            &f,
            &json!({"patch":{"alternatives":with_cue}}),
            &mut budget(),
        )
        .is_err());
    }
}

#[test]
fn ambiguous_branch_surface_narrows_locally_and_unrepairable_duplicate_group_skips_dispatch() {
    // Ambiguous single-word surface ("điện" appears in both branches) narrows to unique local run.
    let source = "Xe điện hoặc tàu điện chạy.";
    let selection = json!({"claims":[{
        "subject":"Xe điện hoặc tàu điện",
        "predicate":"chạy",
        "arguments":["Xe điện hoặc tàu điện"],
        "alternatives":[{
            "quote":"Xe điện hoặc tàu điện",
            "cue":"hoặc",
            "exclusivity":"unspecified",
            "branches":[{"surface":{"quote":"điện"}},{"surface":{"quote":"tàu điện"}}]
        }]
    }]});
    let (_, validation) = assemble(source, &selection, &mut budget()).unwrap();
    assert!(validation
        .issues
        .contains(&"selection[0].alternatives: branch_scope_or_order".into()));
    let (target, fields) = repair_target(source, &selection, &validation, &mut budget())
        .unwrap()
        .unwrap();
    let choices = repair_choices(
        source,
        &selection,
        target,
        &fields,
        &validation,
        &mut budget(),
    )
    .unwrap();
    assert_eq!(
        choices["alternatives"]["scope_or_order"],
        json!([{"group":0,"reorder":true,"branches":[{"branch":0,"surface":["Xe","Xe điện"],"may_keep":false}]}])
    );
    let mut fixed = selection["claims"][0]["alternatives"].clone();
    fixed[0]["branches"][0]["surface"] = json!({"quote":"Xe điện"});
    let (_, _, after) = apply_repair(
        source,
        &selection,
        target,
        &fields,
        &json!({"patch":{"alternatives":fixed}}),
        &mut budget(),
    )
    .unwrap();
    assert!(after.clear(), "{:?} {:?}", after.issues, after.uncovered);

    // Still-reversed reply on two broken branches is rejected even when quotes come from choices.
    let two_bad_src = "Máy dùng pin hoặc điện.";
    let two_bad = json!({"claims":[{
        "subject":"Máy",
        "predicate":"dùng",
        "arguments":["pin hoặc điện"],
        "alternatives":[{
            "quote":"pin hoặc điện",
            "cue":"hoặc",
            "exclusivity":"unspecified",
            "branches":[{"surface":{"quote":"Máy"}},{"surface":{"quote":"dùng"}}]
        }]
    }]});
    let (_, v_bad) = assemble(two_bad_src, &two_bad, &mut budget()).unwrap();
    let (t_bad, f_bad) = repair_target(two_bad_src, &two_bad, &v_bad, &mut budget())
        .unwrap()
        .unwrap();
    let mut wrong_order = two_bad["claims"][0]["alternatives"].clone();
    wrong_order[0]["branches"] = json!([{"surface":{"quote":"điện"}},{"surface":{"quote":"pin"}}]);
    assert!(apply_repair(
        two_bad_src,
        &two_bad,
        t_bad,
        &f_bad,
        &json!({"patch":{"alternatives":wrong_order}}),
        &mut budget(),
    )
    .is_err());

    // Duplicate-token group ("bi hoặc bi") has no unique local token run -> does not dispatch.
    let dup_src = "Hộp có bi hoặc bi.";
    let dup = json!({"claims":[{
        "subject":"Hộp",
        "predicate":"có",
        "arguments":["bi hoặc bi"],
        "alternatives":[{
            "quote":"bi hoặc bi",
            "cue":"hoặc",
            "exclusivity":"unspecified",
            "branches":[{"surface":{"quote":"bi"}},{"surface":{"quote":"bi"}}]
        }]
    }]});
    let (_, v_dup) = assemble(dup_src, &dup, &mut budget()).unwrap();
    assert!(v_dup
        .issues
        .contains(&"selection[0].alternatives: branch_scope_or_order".into()));
    assert!(repair_target(dup_src, &dup, &v_dup, &mut budget())
        .unwrap()
        .is_none());
}

#[test]
fn incompatible_or_cue_dispatches_only_whole_group_drop_keeping_exact_argument() {
    // D-064: a conjunction emitted as an OR group may only be omitted as a whole.
    let source = "Hộp chứa bi xanh và bi đỏ.";
    let selection = json!({"claims":[{"subject":"Hộp","predicate":"chứa","arguments":["bi xanh và bi đỏ"],"alternatives":[{"quote":"bi xanh và bi đỏ","cue":"và","exclusivity":"unspecified","branches":[{"surface":{"quote":"bi xanh"}},{"surface":{"quote":"bi đỏ"}}]}]}]});
    let (_, validation) = assemble(source, &selection, &mut budget()).unwrap();
    assert_eq!(
        validation.issues,
        vec!["selection[0].alternatives: incompatible_or_cue"]
    );
    let (target, fields) = repair_target(source, &selection, &validation, &mut budget())
        .unwrap()
        .unwrap();
    assert_eq!((target, fields.clone()), (0, vec!["alternatives".into()]));
    let choices = repair_choices(
        source,
        &selection,
        target,
        &fields,
        &validation,
        &mut budget(),
    )
    .unwrap();
    assert_eq!(
        choices["alternatives"],
        json!({"groups":1,"repairable":[],"borrowed":[],"drop":[0],"surfaces":["bi xanh","bi đỏ"]})
    );
    let wire: Value =
        serde_json::from_str(&repair_wire_schema(source, &json!(fields), &choices).unwrap())
            .unwrap();
    let alt = &wire["properties"]["patch"]["properties"]["alternatives"];
    assert_eq!(
        (alt["minItems"].clone(), alt["maxItems"].clone()),
        (json!(0), json!(1))
    );
    assert_eq!(
        alt["items"]["properties"]["branches"]["items"]["properties"]["surface"]["properties"]
            ["quote"]["enum"],
        json!(["bi xanh", "bi đỏ"])
    );
    // Accepted: omit the group; the exact quote stays the bound argument.
    let (changed, draft, after) = apply_repair(
        source,
        &selection,
        target,
        &fields,
        &json!({"patch":{"alternatives":[]}}),
        &mut budget(),
    )
    .unwrap();
    assert!(after.clear(), "{:?} {:?}", after.issues, after.uncovered);
    assert_eq!(changed["claims"][0]["alternatives"], json!([]));
    assert_eq!(
        changed["claims"][0]["arguments"],
        json!(["bi xanh và bi đỏ"])
    );
    assert!(draft["statements"][0]["alternatives"]
        .as_array()
        .unwrap()
        .is_empty());
    // Rejected: unchanged, re-labelled cue, dropped branch, substituted group.
    let reject = |alternatives: Value| {
        apply_repair(
            source,
            &selection,
            target,
            &fields,
            &json!({"patch":{"alternatives":alternatives}}),
            &mut budget(),
        )
        .is_err()
    };
    let groups = selection["claims"][0]["alternatives"].clone();
    assert!(reject(groups.clone()));
    let mut relabelled = groups.clone();
    relabelled[0]["cue"] = json!("hoặc");
    assert!(reject(relabelled));
    let mut one_branch = groups.clone();
    one_branch[0]["branches"].as_array_mut().unwrap().pop();
    assert!(reject(one_branch));
    let mut substituted = groups.clone();
    substituted[0]["quote"] = json!("bi xanh");
    assert!(reject(substituted));

    // Unrouted: OR cue inside the quote, unbound quote, referenced group.
    let with_or = json!({"claims":[{"subject":"Hộp","predicate":"chứa","arguments":["bi xanh và bi đỏ hoặc bi vàng"],"alternatives":[{"quote":"bi xanh và bi đỏ hoặc bi vàng","cue":"và","exclusivity":"unspecified","branches":[{"surface":{"quote":"bi xanh"}},{"surface":{"quote":"bi đỏ"}}]}]}]});
    assert!(incompatible_cue_drops(
        "Hộp chứa bi xanh và bi đỏ hoặc bi vàng.",
        &with_or,
        0,
        &mut budget()
    )
    .unwrap()
    .is_empty());
    let mut unbound = selection.clone();
    unbound["claims"][0]["arguments"] = json!(["bi xanh"]);
    assert!(incompatible_cue_drops(source, &unbound, 0, &mut budget())
        .unwrap()
        .is_empty());
    let ref_source = "Hộp chứa bi xanh và bi đỏ. Chúng nặng.";
    let mut referenced = selection.clone();
    referenced["claims"]
        .as_array_mut()
        .unwrap()
        .push(json!({"subject":"Chúng","predicate":"nặng","references":[{"via":{"quote":"Chúng"},"to":{"quote":"bi xanh và bi đỏ"},"target_kind":"group"}]}));
    assert!(
        incompatible_cue_drops(ref_source, &referenced, 0, &mut budget())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn comparison_binding_and_stray_target_dispatch_only_rebind_or_omission() {
    // D-065: rebind a token-extended property to the exact predicate; omit a
    // target emitted with `target_kind: unspecified`. Nothing becomes explicit.
    let source = "Xe đỏ nhanh hơn xe xanh.";
    let selection = json!({"claims":[{"subject":"Xe đỏ","predicate":"nhanh","arguments":["xe xanh"],"comparisons":[{"property":"nhanh hơn","cue":"hơn","target_kind":"unspecified","target":{"quote":"xe xanh"}}]}]});
    let (_, validation) = assemble(source, &selection, &mut budget()).unwrap();
    assert_eq!(
        validation.issues,
        vec![
            "selection[0].comparisons: property_binding",
            "selection[0].comparisons: target_unspecified",
            "selection[0].comparisons: unexpected_target",
        ]
    );
    let (target, fields) = repair_target(source, &selection, &validation, &mut budget())
        .unwrap()
        .unwrap();
    assert_eq!((target, fields.clone()), (0, vec!["comparisons".into()]));
    let choices = repair_choices(
        source,
        &selection,
        target,
        &fields,
        &validation,
        &mut budget(),
    )
    .unwrap();
    assert_eq!(
        choices["comparisons"],
        json!({"count":1,"property":["nhanh","nhanh hơn"],"target_kind":["unspecified"],"targets":["xe xanh"],"repairable":[{"comparison":0,"property":"nhanh","omit_target":true}]})
    );
    let wire: Value =
        serde_json::from_str(&repair_wire_schema(source, &json!(fields), &choices).unwrap())
            .unwrap();
    let cmp = &wire["properties"]["patch"]["properties"]["comparisons"];
    assert_eq!(
        (cmp["minItems"].clone(), cmp["maxItems"].clone()),
        (json!(1), json!(1))
    );
    let props = &cmp["items"]["properties"];
    assert_eq!(props["property"]["enum"], json!(["nhanh", "nhanh hơn"]));
    assert_eq!(props["target_kind"]["enum"], json!(["unspecified"]));
    assert_eq!(
        props["target"]["properties"]["quote"]["enum"],
        json!(["xe xanh"])
    );
    // Accepted: rebind and omit; the unspecified target stays visible.
    let (changed, draft, after) = apply_repair(
        source,
        &selection,
        target,
        &fields,
        &json!({"patch":{"comparisons":[{"property":"nhanh","cue":"hơn","target_kind":"unspecified"}]}}),
        &mut budget(),
    )
    .unwrap();
    assert_eq!(
        after.issues,
        vec!["selection[0].comparisons: target_unspecified"]
    );
    assert!(after.uncovered.is_empty(), "{:?}", after.uncovered);
    assert_eq!(changed["claims"][0]["arguments"], json!(["xe xanh"]));
    assert!(draft["statements"][0]["comparisons"][0]
        .get("target")
        .is_none());
    // Rejected: marked explicit, other property, new target, dropped comparison, cue.
    let reject = |comparisons: Value| {
        apply_repair(
            source,
            &selection,
            target,
            &fields,
            &json!({"patch":{"comparisons":comparisons}}),
            &mut budget(),
        )
        .is_err()
    };
    let base = selection["claims"][0]["comparisons"].clone();
    assert!(reject(base.clone()));
    let mut explicit = base.clone();
    explicit[0]["target_kind"] = json!("explicit");
    assert!(reject(explicit));
    let mut other_property = base.clone();
    other_property[0]["property"] = json!("xe xanh");
    assert!(reject(other_property));
    let mut new_target = base.clone();
    new_target[0]["target"] = json!({"quote":"Xe đỏ"});
    assert!(reject(new_target));
    assert!(reject(json!([])));
    let mut cue = base.clone();
    cue[0]["cue"] = json!("nhanh hơn");
    assert!(reject(cue));

    // Unrouted: property that is not a whitespace-bounded token extension.
    for property in ["xe xanh", "nhanhh"] {
        let unrelated = json!({"claims":[{"subject":"Xe đỏ","predicate":"nhanh","arguments":["xe xanh"],"comparisons":[{"property":property,"cue":"hơn","target_kind":"explicit","target":{"quote":"xe xanh"}}]}]});
        let (_, v) = assemble(source, &unrelated, &mut budget()).unwrap();
        assert_eq!(v.issues, vec!["selection[0].comparisons: property_binding"]);
        assert!(repair_target(source, &unrelated, &v, &mut budget())
            .unwrap()
            .is_none());
    }
}

#[test]
fn ellipsis_role_binding_dispatches_only_bound_rebind_or_omission() {
    // D-066: a flagged ellipsis surface may only become an exact bound
    // token-extended subject/argument, or the flagged ellipsis is omitted.
    let source = "An mua táo đỏ , Bình mua xanh .";
    let ok = json!({"surface":{"quote":"xanh"},"borrowed":{"quote":"táo"}});
    let flagged = json!({"surface":{"quote":"mua xanh"},"borrowed":{"quote":"táo"}});
    let claims = |ellipses: Value| {
        json!({"claims":[
            {"subject":"An","predicate":"mua","arguments":["táo đỏ"],"anchor":"An mua táo đỏ"},
            {"subject":"Bình","predicate":"mua","arguments":["xanh"],"anchor":"Bình mua xanh","ellipses":ellipses}]})
    };
    let selection = claims(json!([flagged]));
    let (_, validation) = assemble(source, &selection, &mut budget()).unwrap();
    assert_eq!(
        validation.issues,
        vec!["selection[1].ellipses: role_binding"]
    );
    let (target, fields) = repair_target(source, &selection, &validation, &mut budget())
        .unwrap()
        .unwrap();
    assert_eq!((target, fields.clone()), (1, vec!["ellipses".into()]));
    let choices = repair_choices(
        source,
        &selection,
        target,
        &fields,
        &validation,
        &mut budget(),
    )
    .unwrap();
    assert_eq!(
        choices["ellipses"],
        json!({"count":1,"surfaces":["mua xanh","xanh"],"borrowed":["táo"],"repairable":[{"ellipsis":0,"surface":["xanh"],"may_omit":true}]})
    );
    let wire: Value =
        serde_json::from_str(&repair_wire_schema(source, &json!(fields), &choices).unwrap())
            .unwrap();
    let ell = &wire["properties"]["patch"]["properties"]["ellipses"];
    assert_eq!(
        (ell["minItems"].clone(), ell["maxItems"].clone()),
        (json!(0), json!(1))
    );
    assert_eq!(
        ell["items"]["properties"]["surface"]["properties"]["quote"]["enum"],
        json!(["mua xanh", "xanh"])
    );
    assert_eq!(
        ell["items"]["properties"]["borrowed"]["properties"]["quote"]["enum"],
        json!(["táo"])
    );
    let apply = |sel: &Value, ellipses: Value| {
        apply_repair(
            source,
            sel,
            target,
            &fields,
            &json!({"patch":{"ellipses":ellipses}}),
            &mut budget(),
        )
    };
    // Accepted: rebind to the bound argument; borrowed text stays reconstructed.
    let (changed, draft, after) = apply(&selection, json!([ok])).unwrap();
    assert!(after.issues.is_empty(), "{:?}", after.issues);
    assert!(after.uncovered.is_empty(), "{:?}", after.uncovered);
    assert_eq!(changed["claims"][1]["arguments"], json!(["xanh"]));
    assert_eq!(
        draft["statements"][1]["ellipses"][0]["origin"],
        "reconstructed"
    );
    // Accepted: omit the flagged ellipsis; the argument stays explicit.
    let (changed, _, after) = apply(&selection, json!([])).unwrap();
    assert!(after.issues.is_empty(), "{:?}", after.issues);
    assert_eq!(changed["claims"][1]["ellipses"], json!([]));
    // Rejected: no-op, unbound/other surface, changed borrowed, selector, extra item.
    assert!(apply(&selection, json!([flagged])).is_err());
    for surface in [json!({"quote":"Bình"}), json!({"quote":"mua"})] {
        assert!(apply(
            &selection,
            json!([{"surface":surface,"borrowed":{"quote":"táo"}}])
        )
        .is_err());
    }
    assert!(apply(
        &selection,
        json!([{"surface":{"quote":"xanh"},"borrowed":{"quote":"táo đỏ"}}])
    )
    .is_err());
    assert!(apply(
        &selection,
        json!([{"surface":{"quote":"xanh","within":"Bình mua xanh"},"borrowed":{"quote":"táo"}}])
    )
    .is_err());
    assert!(apply(&selection, json!([ok, ok])).is_err());
    // Unflagged ellipses stay: only the flagged one may be omitted.
    let mixed = claims(json!([ok, flagged]));
    let (_, v) = assemble(source, &mixed, &mut budget()).unwrap();
    assert_eq!(v.issues, vec!["selection[1].ellipses: role_binding"]);
    assert!(apply(&mixed, json!([ok])).is_ok());
    assert!(apply(&mixed, json!([flagged])).is_err());
    assert!(apply(&mixed, json!([])).is_err());
}

#[test]
fn ellipsis_self_reference_dispatches_only_omission() {
    // D-068: a bound ellipsis whose surface is its own borrowed span may only be
    // omitted; the argument stays explicit, nothing is substituted.
    let source = "An mua táo đỏ , Bình mua xanh .";
    let ok = json!({"surface":{"quote":"xanh"},"borrowed":{"quote":"táo"}});
    let selfref = json!({"surface":{"quote":"xanh"},"borrowed":{"quote":"xanh"}});
    let claims = |ellipses: Value| {
        json!({"claims":[
            {"subject":"An","predicate":"mua","arguments":["táo đỏ"],"anchor":"An mua táo đỏ"},
            {"subject":"Bình","predicate":"mua","arguments":["xanh"],"anchor":"Bình mua xanh","ellipses":ellipses}]})
    };
    let selection = claims(json!([selfref]));
    let (_, validation) = assemble(source, &selection, &mut budget()).unwrap();
    assert_eq!(
        validation.issues,
        vec!["selection[1].ellipses: self_reference"]
    );
    let (target, fields) = repair_target(source, &selection, &validation, &mut budget())
        .unwrap()
        .unwrap();
    assert_eq!((target, fields.clone()), (1, vec!["ellipses".into()]));
    let choices = repair_choices(
        source,
        &selection,
        target,
        &fields,
        &validation,
        &mut budget(),
    )
    .unwrap();
    assert_eq!(
        choices["ellipses"],
        json!({"count":1,"surfaces":["xanh"],"borrowed":["xanh"],"repairable":[{"ellipsis":0,"surface":[],"may_omit":true}]})
    );
    let wire: Value =
        serde_json::from_str(&repair_wire_schema(source, &json!(fields), &choices).unwrap())
            .unwrap();
    let ell = &wire["properties"]["patch"]["properties"]["ellipses"];
    assert_eq!(
        (ell["minItems"].clone(), ell["maxItems"].clone()),
        (json!(0), json!(1))
    );
    let apply = |sel: &Value, ellipses: Value| {
        apply_repair(
            source,
            sel,
            target,
            &fields,
            &json!({"patch":{"ellipses":ellipses}}),
            &mut budget(),
        )
    };
    // Accepted: omit; the argument stays an explicit quote, nothing reconstructed.
    let (changed, draft, after) = apply(&selection, json!([])).unwrap();
    assert!(after.issues.is_empty(), "{:?}", after.issues);
    assert!(after.uncovered.is_empty(), "{:?}", after.uncovered);
    assert_eq!(changed["claims"][1]["arguments"], json!(["xanh"]));
    assert_eq!(draft["statements"][1]["ellipses"], json!([]));
    // Rejected: no-op, substituted borrowed text (even a valid one), other surface.
    assert!(apply(&selection, json!([selfref])).is_err());
    assert!(apply(&selection, json!([ok])).is_err());
    assert!(apply(
        &selection,
        json!([{"surface":{"quote":"Bình"},"borrowed":{"quote":"xanh"}}])
    )
    .is_err());
    // Unflagged ellipses stay byte-identical: only the self-reference may go.
    let mixed = claims(json!([ok, selfref]));
    let (_, v) = assemble(source, &mixed, &mut budget()).unwrap();
    assert_eq!(v.issues, vec!["selection[1].ellipses: self_reference"]);
    assert!(apply(&mixed, json!([ok])).is_ok());
    assert!(apply(&mixed, json!([selfref])).is_err());
    assert!(apply(&mixed, json!([])).is_err());
    // Unrouted: a self-referencing borrowed span that does not resolve uniquely.
    let ambiguous = json!({"claims":[{"subject":"Bình","predicate":"mua","arguments":["mua"],"ellipses":[{"surface":{"quote":"mua"},"borrowed":{"quote":"mua"}}]}]});
    let (_, v) = assemble(source, &ambiguous, &mut budget()).unwrap();
    assert!(!v
        .issues
        .contains(&"selection[0].ellipses: self_reference".to_string()));
    assert!(
        ellipsis_choices(source, &ambiguous["claims"][0], &mut budget())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn comparison_unresolved_target_dispatches_only_unspecified_downgrade() {
    // D-067: an explicit/implicit target that does not resolve uniquely may only
    // become `target_kind: unspecified` with that target omitted; nothing invented.
    let source = "Xe đỏ nhanh hơn xe xanh.";
    let selection_with = |kind: &str| json!({"claims":[{"subject":"Xe đỏ","predicate":"nhanh","arguments":["xe xanh"],"comparisons":[{"property":"nhanh","cue":"hơn","target_kind":kind,"target":{"quote":"xe tải"}}]}]});
    let selection = selection_with("explicit");
    let (_, validation) = assemble(source, &selection, &mut budget()).unwrap();
    assert_eq!(
        validation.issues,
        vec!["selection[0].comparisons: target_absent_or_ambiguous"]
    );
    let (target, fields) = repair_target(source, &selection, &validation, &mut budget())
        .unwrap()
        .unwrap();
    assert_eq!((target, fields.clone()), (0, vec!["comparisons".into()]));
    let choices = repair_choices(
        source,
        &selection,
        target,
        &fields,
        &validation,
        &mut budget(),
    )
    .unwrap();
    assert_eq!(
        choices["comparisons"],
        json!({"count":1,"property":["nhanh"],"target_kind":["explicit","unspecified"],"targets":["xe tải"],"repairable":[{"comparison":0,"target_kind":"unspecified","omit_target":true}]})
    );
    let wire: Value =
        serde_json::from_str(&repair_wire_schema(source, &json!(fields), &choices).unwrap())
            .unwrap();
    let props = &wire["properties"]["patch"]["properties"]["comparisons"]["items"]["properties"];
    assert_eq!(
        props["target_kind"]["enum"],
        json!(["explicit", "unspecified"])
    );
    let apply = |sel: &Value, comparisons: Value| {
        apply_repair(
            source,
            sel,
            target,
            &fields,
            &json!({"patch":{"comparisons":comparisons}}),
            &mut budget(),
        )
    };
    let downgraded = json!([{"property":"nhanh","cue":"hơn","target_kind":"unspecified"}]);
    // Accepted: downgrade + omit; only `target_unspecified` remains visible.
    let (changed, draft, after) = apply(&selection, downgraded.clone()).unwrap();
    assert_eq!(
        after.issues,
        vec!["selection[0].comparisons: target_unspecified"]
    );
    assert!(after.uncovered.is_empty(), "{:?}", after.uncovered);
    assert_eq!(changed["claims"][0]["arguments"], json!(["xe xanh"]));
    let cmp = &draft["statements"][0]["comparisons"][0];
    assert!(cmp.get("target").is_none() && cmp.get("target_span").is_none());
    // Implicit candidate: downgrade also clears the unassessed inference.
    let implicit = selection_with("implicit_candidate");
    let (_, v) = assemble(source, &implicit, &mut budget()).unwrap();
    assert_eq!(
        v.issues,
        vec![
            "selection[0].comparisons: implicit_target_unassessed",
            "selection[0].comparisons: target_absent_or_ambiguous",
        ]
    );
    let (_, _, after) = apply(&implicit, downgraded).unwrap();
    assert_eq!(
        after.issues,
        vec!["selection[0].comparisons: target_unspecified"]
    );
    // Rejected: no-op, kept target, target dropped without downgrade, other kind,
    // invented resolvable target, dropped comparison, changed cue.
    let base = selection["claims"][0]["comparisons"].clone();
    assert!(apply(&selection, base.clone()).is_err());
    let mut kept_target = base.clone();
    kept_target[0]["target_kind"] = json!("unspecified");
    assert!(apply(&selection, kept_target).is_err());
    let mut dropped_only = base.clone();
    dropped_only[0].as_object_mut().unwrap().remove("target");
    assert!(apply(&selection, dropped_only).is_err());
    let mut other_kind = base.clone();
    other_kind[0]["target_kind"] = json!("implicit_candidate");
    other_kind[0].as_object_mut().unwrap().remove("target");
    assert!(apply(&selection, other_kind).is_err());
    let mut invented = base.clone();
    invented[0]["target"] = json!({"quote":"xe xanh"});
    assert!(apply(&selection, invented).is_err());
    assert!(apply(&selection, json!([])).is_err());
    let mut cue = base.clone();
    cue[0]["cue"] = json!("nhanh hơn");
    cue[0]["target_kind"] = json!("unspecified");
    cue[0].as_object_mut().unwrap().remove("target");
    assert!(apply(&selection, cue).is_err());

    // Unrouted: an unresolved cue alone is not a target repair.
    let cue_only = json!({"claims":[{"subject":"Xe đỏ","predicate":"nhanh","arguments":["xe xanh"],"comparisons":[{"property":"nhanh","cue":"kém","target_kind":"explicit","target":{"quote":"xe xanh"}}]}]});
    let (_, v) = assemble(source, &cue_only, &mut budget()).unwrap();
    assert_eq!(v.issues, vec!["selection[0].comparisons: cue_ambiguous"]);
    assert!(repair_target(source, &cue_only, &v, &mut budget())
        .unwrap()
        .is_none());
}

#[test]
fn comparison_explicit_target_scope_dispatches_only_unspecified_downgrade() {
    // D-069: a resolved explicit target outside the claim's evidence span, or
    // inside another claim's term, may only become `target_kind: unspecified`
    // with that target omitted; nothing invented, substituted or re-scoped.
    let source = "Xe đỏ nhanh hơn, còn xe tải thì chậm.";
    let selection = json!({"claims":[{"subject":"Xe đỏ","predicate":"nhanh","comparisons":[{"property":"nhanh","cue":"hơn","target_kind":"explicit","target":{"quote":"xe tải"}}]}]});
    let (_, validation) = assemble(source, &selection, &mut budget()).unwrap();
    assert_eq!(
        validation.issues,
        vec!["selection[0].comparisons: explicit_target_scope"]
    );
    let (target, fields) = repair_target(source, &selection, &validation, &mut budget())
        .unwrap()
        .unwrap();
    assert_eq!((target, fields.clone()), (0, vec!["comparisons".into()]));
    let choices = repair_choices(
        source,
        &selection,
        target,
        &fields,
        &validation,
        &mut budget(),
    )
    .unwrap();
    assert_eq!(
        choices["comparisons"],
        json!({"count":1,"property":["nhanh"],"target_kind":["explicit","unspecified"],"targets":["xe tải"],"repairable":[{"comparison":0,"target_kind":"unspecified","omit_target":true}]})
    );
    let apply = |sel: &Value, comparisons: Value| {
        apply_repair(
            source,
            sel,
            target,
            &fields,
            &json!({"patch":{"comparisons":comparisons}}),
            &mut budget(),
        )
    };
    let downgraded = json!([{"property":"nhanh","cue":"hơn","target_kind":"unspecified"}]);
    let (_, draft, after) = apply(&selection, downgraded.clone()).unwrap();
    assert_eq!(
        after.issues,
        vec!["selection[0].comparisons: target_unspecified"]
    );
    assert_eq!(after.uncovered, validation.uncovered);
    let cmp = &draft["statements"][0]["comparisons"][0];
    assert!(cmp.get("target").is_none() && cmp.get("target_span").is_none());
    // Rejected: no-op, kept target, target dropped without downgrade, other
    // kind, substituted in-scope target, dropped comparison, changed cue.
    let base = selection["claims"][0]["comparisons"].clone();
    assert!(apply(&selection, base.clone()).is_err());
    let mut kept_target = base.clone();
    kept_target[0]["target_kind"] = json!("unspecified");
    assert!(apply(&selection, kept_target).is_err());
    let mut dropped_only = base.clone();
    dropped_only[0].as_object_mut().unwrap().remove("target");
    assert!(apply(&selection, dropped_only).is_err());
    let mut other_kind = base.clone();
    other_kind[0]["target_kind"] = json!("implicit_candidate");
    other_kind[0].as_object_mut().unwrap().remove("target");
    assert!(apply(&selection, other_kind).is_err());
    let mut substituted = base.clone();
    substituted[0]["target"] = json!({"quote":"Xe đỏ"});
    assert!(apply(&selection, substituted).is_err());
    assert!(apply(&selection, json!([])).is_err());
    let mut cue = downgraded.clone();
    cue[0]["cue"] = json!("nhanh hơn");
    assert!(apply(&selection, cue).is_err());

    // Inside another claim's term (raised after all claim roles exist): only
    // the flagged comparison downgrades; the other claim stays untouched.
    let source = "Xe đỏ nhanh hơn xe xanh cũ.";
    let selection = json!({"claims":[
        {"subject":"Xe đỏ","predicate":"nhanh","arguments":["xe xanh cũ"],"comparisons":[{"property":"nhanh","cue":"hơn","target_kind":"explicit","target":{"quote":"xe xanh"}}]},
        {"subject":"xe xanh","predicate":"cũ"}
    ]});
    let (_, validation) = assemble(source, &selection, &mut budget()).unwrap();
    assert!(validation
        .issues
        .contains(&"selection[0].comparisons: explicit_target_scope".into()));
    let (target, fields) = repair_target(source, &selection, &validation, &mut budget())
        .unwrap()
        .unwrap();
    assert_eq!((target, fields.clone()), (0, vec!["comparisons".into()]));
    let (changed, _, after) = apply_repair(
        source,
        &selection,
        target,
        &fields,
        &json!({"patch":{"comparisons":downgraded}}),
        &mut budget(),
    )
    .unwrap();
    assert!(!after
        .issues
        .contains(&"selection[0].comparisons: explicit_target_scope".into()));
    assert_eq!(changed["claims"][1], selection["claims"][1]);
    assert_eq!(changed["claims"][0]["arguments"], json!(["xe xanh cũ"]));

    // Unrouted: an in-scope explicit target owned by no other claim.
    let source = "Xe đỏ nhanh hơn xe xanh.";
    let in_scope = json!({"claims":[{"subject":"Xe đỏ","predicate":"nhanh","arguments":["xe xanh"],"comparisons":[{"property":"nhanh","cue":"hơn","target_kind":"explicit","target":{"quote":"xe xanh"}}]}]});
    let (_, v) = assemble(source, &in_scope, &mut budget()).unwrap();
    assert!(v.issues.is_empty(), "{:?}", v.issues);
    assert!(comparison_choices(source, &in_scope, 0, &mut budget())
        .unwrap()
        .is_empty());
}

#[test]
fn comparison_cue_ambiguous_dispatches_only_local_token_run_narrowing() {
    // D-070: an unresolved comparison `cue` may only narrow to a contiguous
    // whitespace-bounded token run of the emitted `cue` that resolves uniquely
    // in the claim scope; unrepairable cues do not dispatch.
    let source = "Xe đỏ nhanh hơn xe xanh.";
    let selection = json!({"claims":[{"subject":"Xe đỏ","predicate":"nhanh","arguments":["xe xanh"],"comparisons":[{"property":"nhanh","cue":"hơn so với","target_kind":"explicit","target":{"quote":"xe xanh"}}]}]});
    let (_, validation) = assemble(source, &selection, &mut budget()).unwrap();
    assert_eq!(
        validation.issues,
        vec!["selection[0].comparisons: cue_ambiguous"]
    );
    let (target, fields) = repair_target(source, &selection, &validation, &mut budget())
        .unwrap()
        .unwrap();
    assert_eq!((target, fields.clone()), (0, vec!["comparisons".into()]));
    let choices = repair_choices(
        source,
        &selection,
        target,
        &fields,
        &validation,
        &mut budget(),
    )
    .unwrap();
    assert_eq!(
        choices["comparisons"],
        json!({"count":1,"cue":["hơn"],"property":["nhanh"],"target_kind":["explicit"],"targets":["xe xanh"],"repairable":[{"comparison":0,"cue":["hơn"]}]})
    );
    let wire: Value =
        serde_json::from_str(&repair_wire_schema(source, &json!(fields), &choices).unwrap())
            .unwrap();
    let props = &wire["properties"]["patch"]["properties"]["comparisons"]["items"]["properties"];
    assert_eq!(props["cue"]["enum"], json!(["hơn"]));
    let apply = |sel: &Value, comparisons: Value| {
        apply_repair(
            source,
            sel,
            target,
            &fields,
            &json!({"patch":{"comparisons":comparisons}}),
            &mut budget(),
        )
    };
    let narrowed = json!([{"property":"nhanh","cue":"hơn","target_kind":"explicit","target":{"quote":"xe xanh"}}]);
    let (changed, draft, after) = apply(&selection, narrowed).unwrap();
    assert!(after.issues.is_empty(), "{:?}", after.issues);
    assert!(after.uncovered.is_empty(), "{:?}", after.uncovered);
    assert_eq!(changed["claims"][0]["arguments"], json!(["xe xanh"]));
    assert!(draft["statements"][0]["comparisons"][0]["cue_span"].is_array());

    // Rejected: no-op, token outside emitted cue, non-whitespace character slice,
    // altered target/kind/property, or dropped comparison.
    let base = selection["claims"][0]["comparisons"].clone();
    assert!(apply(&selection, base.clone()).is_err());
    for bad_cue in ["nhanh hơn", "hơ", "ơ"] {
        let mut bad = base.clone();
        bad[0]["cue"] = json!(bad_cue);
        assert!(apply(&selection, bad).is_err());
    }
    let mut downgraded = base.clone();
    downgraded[0]["cue"] = json!("hơn");
    downgraded[0]["target_kind"] = json!("unspecified");
    downgraded[0].as_object_mut().unwrap().remove("target");
    assert!(apply(&selection, downgraded).is_err());
    assert!(apply(&selection, json!([])).is_err());

    // Unflagged sibling comparison keeps its exact cue; only the flagged one narrows.
    let source2 = "Xe đỏ nhanh hơn xe xanh và bền nhất đội.";
    let mixed = json!({"claims":[{"subject":"Xe đỏ","predicate":"nhanh","arguments":["xe xanh","bền nhất đội"],"comparisons":[
        {"property":"nhanh","cue":"hơn","target_kind":"explicit","target":{"quote":"xe xanh"}},
        {"property":"nhanh","cue":"nhất cả","target_kind":"explicit","target":{"quote":"bền nhất đội"}}
    ]}]});
    let (_, v2) = assemble(source2, &mixed, &mut budget()).unwrap();
    assert_eq!(v2.issues, vec!["selection[0].comparisons: cue_ambiguous"]);
    let choices2 = repair_choices(source2, &mixed, 0, &fields, &v2, &mut budget()).unwrap();
    assert_eq!(choices2["comparisons"]["cue"], json!(["hơn", "nhất"]));
    assert!(apply_repair(
        source2,
        &mixed,
        0,
        &fields,
        &json!({"patch":{"comparisons":[
            {"property":"nhanh","cue":"hơn","target_kind":"explicit","target":{"quote":"xe xanh"}},
            {"property":"nhanh","cue":"nhất","target_kind":"explicit","target":{"quote":"bền nhất đội"}}
        ]}}),
        &mut budget(),
    )
    .is_ok());
    // Swapping the unflagged comparison's cue to the sibling's narrowed cue is rejected.
    assert!(apply_repair(
        source2,
        &mixed,
        0,
        &fields,
        &json!({"patch":{"comparisons":[
            {"property":"nhanh","cue":"nhất","target_kind":"explicit","target":{"quote":"xe xanh"}},
            {"property":"nhanh","cue":"nhất","target_kind":"explicit","target":{"quote":"bền nhất đội"}}
        ]}}),
        &mut budget(),
    )
    .is_err());

    // Unrouted: duplicate single-token cue in scope has no unique token run.
    let dup_source = "Xe đỏ nhanh hơn xe xanh, xe vàng cao hơn xe trắng.";
    let dup_sel = json!({"claims":[
        {"subject":"Xe đỏ","predicate":"nhanh","arguments":["xe xanh"],"comparisons":[{"property":"nhanh","cue":"hơn","target_kind":"explicit","target":{"quote":"xe xanh"}}]},
        {"subject":"xe vàng","predicate":"cao","arguments":["xe trắng"],"anchor":"xe vàng cao hơn xe trắng"}
    ]});
    let (_, dup_v) = assemble(dup_source, &dup_sel, &mut budget()).unwrap();
    assert_eq!(
        dup_v.issues,
        vec!["selection[0].comparisons: cue_ambiguous"]
    );
    assert!(comparison_choices(dup_source, &dup_sel, 0, &mut budget())
        .unwrap()
        .is_empty());
    assert!(repair_target(dup_source, &dup_sel, &dup_v, &mut budget())
        .unwrap()
        .is_none());
}

#[test]
fn alternatives_cue_ambiguous_dispatches_only_local_token_run_narrowing() {
    // D-071: an unresolved OR-group `cue` may only narrow to a contiguous
    // whitespace-bounded token run of the emitted `cue` that resolves uniquely
    // inside `group.quote` outside located branch spans; unrepairable cues do
    // not dispatch.
    let source = "Máy dùng pin hoặc điện.";
    let selection = json!({"claims":[{"subject":"Máy","predicate":"dùng","arguments":["pin hoặc điện"],"alternatives":[{"quote":"pin hoặc điện","cue":"hoặc là pin","exclusivity":"unspecified","branches":[{"surface":{"quote":"pin"}},{"surface":{"quote":"điện"}}]}]}]});
    let (_, validation) = assemble(source, &selection, &mut budget()).unwrap();
    assert_eq!(
        validation.issues,
        vec!["selection[0].alternatives: cue_ambiguous"]
    );
    assert_eq!(validation.uncovered, vec!["hoặc"]);
    let (target, fields) = repair_target(source, &selection, &validation, &mut budget())
        .unwrap()
        .unwrap();
    assert_eq!((target, fields.clone()), (0, vec!["alternatives".into()]));
    let choices = repair_choices(
        source,
        &selection,
        target,
        &fields,
        &validation,
        &mut budget(),
    )
    .unwrap();
    // "pin" is a token of the emitted cue but overlaps a located branch.
    assert_eq!(
        choices["alternatives"],
        json!({"groups":1,"repairable":[],"borrowed":[],"cue_narrowing":[{"group":0,"cue":["hoặc"]}],"cue":["hoặc"]})
    );
    let wire: Value =
        serde_json::from_str(&repair_wire_schema(source, &json!(fields), &choices).unwrap())
            .unwrap();
    let props = &wire["properties"]["patch"]["properties"]["alternatives"]["items"]["properties"];
    assert_eq!(props["cue"]["enum"], json!(["hoặc"]));
    let apply = |sel: &Value, alternatives: Value| {
        apply_repair(
            source,
            sel,
            target,
            &fields,
            &json!({"patch":{"alternatives":alternatives}}),
            &mut budget(),
        )
    };
    let base = selection["claims"][0]["alternatives"].clone();
    let mut narrowed = base.clone();
    narrowed[0]["cue"] = json!("hoặc");
    let (changed, draft, after) = apply(&selection, narrowed).unwrap();
    assert!(after.issues.is_empty(), "{:?}", after.issues);
    assert!(after.uncovered.is_empty(), "{:?}", after.uncovered);
    assert_eq!(changed["claims"][0]["arguments"], json!(["pin hoặc điện"]));
    assert_eq!(
        changed["claims"][0]["alternatives"][0]["branches"],
        base[0]["branches"]
    );
    assert!(draft["statements"][0]["alternatives"][0]["cue_span"].is_array());

    // Rejected: no-op, branch token, token outside emitted cue, sub-word slice,
    // altered exclusivity/quote/branches, or dropped group.
    assert!(apply(&selection, base.clone()).is_err());
    for bad_cue in ["pin", "là", "hoặ", "điện"] {
        let mut bad = base.clone();
        bad[0]["cue"] = json!(bad_cue);
        assert!(apply(&selection, bad).is_err());
    }
    let mut excl = base.clone();
    excl[0]["cue"] = json!("hoặc");
    excl[0]["exclusivity"] = json!("exclusive");
    assert!(apply(&selection, excl).is_err());
    let mut quote = base.clone();
    quote[0]["cue"] = json!("hoặc");
    quote[0]["quote"] = json!("pin hoặc");
    assert!(apply(&selection, quote).is_err());
    let mut branch = base.clone();
    branch[0]["cue"] = json!("hoặc");
    branch[0]["branches"][1]["surface"]["quote"] = json!("điện.");
    assert!(apply(&selection, branch).is_err());
    assert!(apply(&selection, json!([])).is_err());

    // Unflagged sibling group keeps its exact cue; only the flagged one narrows.
    let source2 = "Máy dùng pin hoặc điện, nước hay gió.";
    let mixed = json!({"claims":[{"subject":"Máy","predicate":"dùng","arguments":["pin hoặc điện","nước hay gió"],"alternatives":[
        {"quote":"pin hoặc điện","cue":"hoặc là","exclusivity":"unspecified","branches":[{"surface":{"quote":"pin"}},{"surface":{"quote":"điện"}}]},
        {"quote":"nước hay gió","cue":"hay","exclusivity":"unspecified","branches":[{"surface":{"quote":"nước"}},{"surface":{"quote":"gió"}}]}
    ]}]});
    let (_, v2) = assemble(source2, &mixed, &mut budget()).unwrap();
    assert_eq!(v2.issues, vec!["selection[0].alternatives: cue_ambiguous"]);
    let choices2 = repair_choices(source2, &mixed, 0, &fields, &v2, &mut budget()).unwrap();
    assert_eq!(choices2["alternatives"]["cue"], json!(["hay", "hoặc"]));
    let groups = mixed["claims"][0]["alternatives"].clone();
    let mut ok = groups.clone();
    ok[0]["cue"] = json!("hoặc");
    let apply2 = |alternatives: Value| {
        apply_repair(
            source2,
            &mixed,
            0,
            &fields,
            &json!({"patch":{"alternatives":alternatives}}),
            &mut budget(),
        )
    };
    assert!(apply2(ok.clone()).is_ok());
    // Relabelling the unflagged group's cue is rejected.
    let mut swapped = ok;
    swapped[1]["cue"] = json!("hoặc");
    assert!(apply2(swapped).is_err());

    // Unrouted: repeated single-token cue and absent cue have no unique run.
    for (src, sel) in [
        (
            "Máy dùng pin hoặc điện hoặc gió.",
            json!({"claims":[{"subject":"Máy","predicate":"dùng","arguments":["pin hoặc điện hoặc gió"],"alternatives":[{"quote":"pin hoặc điện hoặc gió","cue":"hoặc","exclusivity":"unspecified","branches":[{"surface":{"quote":"pin"}},{"surface":{"quote":"điện"}},{"surface":{"quote":"gió"}}]}]}]}),
        ),
        (
            "Máy dùng pin hoặc điện.",
            json!({"claims":[{"subject":"Máy","predicate":"dùng","arguments":["pin hoặc điện"],"alternatives":[{"quote":"pin hoặc điện","cue":"hay là","exclusivity":"unspecified","branches":[{"surface":{"quote":"pin"}},{"surface":{"quote":"điện"}}]}]}]}),
        ),
    ] {
        let (_, v) = assemble(src, &sel, &mut budget()).unwrap();
        assert_eq!(v.issues, vec!["selection[0].alternatives: cue_ambiguous"]);
        assert!(alternative_cue_choices(src, &sel, 0, &mut budget())
            .unwrap()
            .is_empty());
        assert!(repair_target(src, &sel, &v, &mut budget())
            .unwrap()
            .is_none());
    }
}

#[test]
fn references_cue_ambiguous_dispatches_only_local_token_run_narrowing() {
    // D-072: an unresolved reference `via` may only narrow to a contiguous
    // whitespace-bounded token run of the emitted `via.quote` that resolves
    // uniquely in the claim scope outside the resolved `to` span; `to`,
    // `target_kind` and `via.within` stay fixed; unrepairable cues do not dispatch.
    let mut selection = rocket();
    selection["claims"][1]["references"][0]["via"] = json!({"quote":"Trong đó ấy"});
    let (_, validation) = assemble(ROCKET, &selection, &mut budget()).unwrap();
    assert_eq!(
        validation.issues,
        vec![
            "selection[1].comparisons: target_unspecified",
            "selection[1].references: cue_ambiguous"
        ]
    );
    assert_eq!(validation.uncovered, vec![". Trong đó ,"]);
    let (target, fields) = repair_target(ROCKET, &selection, &validation, &mut budget())
        .unwrap()
        .unwrap();
    assert_eq!((target, fields.clone()), (1, vec!["references".into()]));
    let choices = repair_choices(
        ROCKET,
        &selection,
        target,
        &fields,
        &validation,
        &mut budget(),
    )
    .unwrap();
    assert_eq!(
        choices["references"],
        json!({"count":1,"via":["Trong","Trong đó","đó"],"to":["nhiên liệu lỏng hoặc rắn"],"target_kind":["group"],"repairable":[{"reference":0,"via":["Trong","Trong đó","đó"]}]})
    );
    let wire: Value =
        serde_json::from_str(&repair_wire_schema(ROCKET, &json!(fields), &choices).unwrap())
            .unwrap();
    let refs = &wire["properties"]["patch"]["properties"]["references"];
    assert_eq!(
        (refs["minItems"].clone(), refs["maxItems"].clone()),
        (json!(1), json!(1))
    );
    assert_eq!(
        refs["items"]["properties"]["via"]["properties"]["quote"]["enum"],
        json!(["Trong", "Trong đó", "đó"])
    );
    assert_eq!(
        refs["items"]["properties"]["to"]["properties"]["quote"]["enum"],
        json!(["nhiên liệu lỏng hoặc rắn"])
    );
    assert_eq!(
        refs["items"]["properties"]["target_kind"]["enum"],
        json!(["group"])
    );
    let apply = |sel: &Value, references: Value| {
        apply_repair(
            ROCKET,
            sel,
            target,
            &fields,
            &json!({"patch":{"references":references}}),
            &mut budget(),
        )
    };
    let base = selection["claims"][1]["references"].clone();
    let mut narrowed = base.clone();
    narrowed[0]["via"]["quote"] = json!("Trong đó");
    let (changed, draft, after) = apply(&selection, narrowed).unwrap();
    assert_eq!(
        after.issues,
        vec!["selection[1].comparisons: target_unspecified"]
    );
    assert!(after.uncovered.is_empty(), "{:?}", after.uncovered);
    assert_eq!(changed["claims"][0], selection["claims"][0]);
    assert_eq!(
        draft["statements"][1]["references"][0]["target_id"],
        "claim-0-group-0"
    );
    assert!(draft["statements"][1]["references"][0]["via_span"].is_array());

    // Rejected: no-op, token outside emitted cue, sub-word slice, altered
    // target/kind, added `within`, or dropped reference.
    assert!(apply(&selection, base.clone()).is_err());
    for bad in ["nhiên liệu", "Tron", "ấy"] {
        let mut r = base.clone();
        r[0]["via"]["quote"] = json!(bad);
        assert!(apply(&selection, r).is_err());
    }
    let mut retarget = base.clone();
    retarget[0]["via"]["quote"] = json!("Trong đó");
    retarget[0]["to"] = json!({"quote":"nhiên liệu lỏng"});
    assert!(apply(&selection, retarget).is_err());
    let mut rekind = base.clone();
    rekind[0]["via"]["quote"] = json!("Trong đó");
    rekind[0]["target_kind"] = json!("claim");
    assert!(apply(&selection, rekind).is_err());
    let mut scoped = base.clone();
    scoped[0]["via"] = json!({"quote":"Trong đó","within":"Trong đó , nhiên liệu rắn"});
    assert!(apply(&selection, scoped).is_err());
    assert!(apply(&selection, json!([])).is_err());

    // An emitted `within` is kept: dropping it is rejected, keeping it is accepted.
    let mut within = rocket();
    within["claims"][1]["references"][0]["via"] =
        json!({"quote":"Trong đó ấy","within":"Trong đó , nhiên liệu rắn"});
    let (_, wv) = assemble(ROCKET, &within, &mut budget()).unwrap();
    assert!(wv
        .issues
        .contains(&"selection[1].references: cue_ambiguous".to_owned()));
    let mut kept = within["claims"][1]["references"].clone();
    kept[0]["via"]["quote"] = json!("Trong đó");
    assert!(apply(&within, kept).is_ok());
    let mut dropped = within["claims"][1]["references"].clone();
    dropped[0]["via"] = json!({"quote":"Trong đó"});
    assert!(apply(&within, dropped).is_err());

    // Unflagged sibling reference keeps its exact `via`; only the flagged one narrows.
    let mut mixed = rocket();
    mixed["claims"][1]["references"]
        .as_array_mut()
        .unwrap()
        .push(json!({"via":{"quote":"sẽ kia"},"to":{"quote":"nhiên liệu lỏng hoặc rắn"},"target_kind":"group"}));
    let (_, mv) = assemble(ROCKET, &mixed, &mut budget()).unwrap();
    assert!(mv
        .issues
        .contains(&"selection[1].references: cue_ambiguous".to_owned()));
    let mc = repair_choices(ROCKET, &mixed, 1, &fields, &mv, &mut budget()).unwrap();
    assert_eq!(mc["references"]["via"], json!(["Trong đó", "sẽ"]));
    let refs = mixed["claims"][1]["references"].clone();
    let mut ok = refs.clone();
    ok[1]["via"]["quote"] = json!("sẽ");
    assert!(apply(&mixed, ok.clone()).is_ok());
    let mut swapped = ok;
    swapped[0]["via"]["quote"] = json!("sẽ");
    assert!(apply(&mixed, swapped).is_err());

    // Unrouted: the only resolving token lies inside the resolved target, and
    // the other token is absent.
    let source = "A chạy. B dừng.";
    let sel = json!({"claims":[{"subject":"A","predicate":"chạy"},{"subject":"B","predicate":"dừng","references":[{"via":{"quote":"chạy kia"},"to":{"quote":"A chạy"},"target_kind":"claim"}]}]});
    let (_, v) = assemble(source, &sel, &mut budget()).unwrap();
    assert!(v
        .issues
        .contains(&"selection[1].references: cue_ambiguous".to_owned()));
    assert!(
        reference_cue_choices(source, &sel["claims"][1], &mut budget())
            .unwrap()
            .is_empty()
    );
    assert!(repair_target(source, &sel, &v, &mut budget())
        .unwrap()
        .is_none());
}

#[test]
fn alternatives_argument_binding_dispatches_only_bound_argument_rebind() {
    // D-073: an OR group flagged `argument_binding` whose emitted `quote`,
    // `cue`, ordered disjoint `branches` and `borrowed` reconstructions resolve
    // cleanly may only rebind `group.quote` to a bound `argument` of the same
    // claim that is a whitespace-bounded token extension and preserves unique
    // resolution without new issues or coverage loss.
    let mut selection = rocket();
    selection["claims"][0]["alternatives"][0]["quote"] = json!("sử dụng nhiên liệu lỏng hoặc rắn");
    let (_, validation) = assemble(ROCKET, &selection, &mut budget()).unwrap();
    assert_eq!(
        validation.issues,
        vec![
            "selection[0].alternatives: argument_binding",
            "selection[0].alternatives: unrepresented_choice_cue",
            "selection[1].comparisons: target_unspecified",
            "selection[1].references: target_absent_ambiguous_or_self"
        ]
    );
    assert!(
        validation.uncovered.is_empty(),
        "{:?}",
        validation.uncovered
    );
    let (target, fields) = repair_target(ROCKET, &selection, &validation, &mut budget())
        .unwrap()
        .unwrap();
    assert_eq!((target, fields.clone()), (0, vec!["alternatives".into()]));
    let choices = repair_choices(
        ROCKET,
        &selection,
        target,
        &fields,
        &validation,
        &mut budget(),
    )
    .unwrap();
    // Unrelated bound argument "có thể" is excluded; reconstructed "nhiên liệu" kept.
    assert_eq!(
        choices["alternatives"],
        json!({
            "groups": 1,
            "repairable": [],
            "borrowed": ["nhiên liệu"],
            "argument_binding": [{"group": 0, "quote": ["nhiên liệu lỏng hoặc rắn"]}],
            "quote": ["nhiên liệu lỏng hoặc rắn"]
        })
    );
    let wire: Value =
        serde_json::from_str(&repair_wire_schema(ROCKET, &json!(fields), &choices).unwrap())
            .unwrap();
    let alt = &wire["properties"]["patch"]["properties"]["alternatives"];
    assert_eq!(
        (alt["minItems"].clone(), alt["maxItems"].clone()),
        (json!(1), json!(1))
    );
    assert_eq!(
        alt["items"]["properties"]["quote"]["enum"],
        json!(["nhiên liệu lỏng hoặc rắn"])
    );
    let apply = |sel: &Value, alternatives: Value| {
        apply_repair(
            ROCKET,
            sel,
            target,
            &fields,
            &json!({"patch":{"alternatives":alternatives}}),
            &mut budget(),
        )
    };
    let base = selection["claims"][0]["alternatives"].clone();
    let mut rebound = base.clone();
    rebound[0]["quote"] = json!("nhiên liệu lỏng hoặc rắn");
    let (changed, draft, after) = apply(&selection, rebound).unwrap();
    assert_eq!(
        after.issues,
        vec!["selection[1].comparisons: target_unspecified"]
    );
    assert!(after.uncovered.is_empty(), "{:?}", after.uncovered);
    assert_eq!(
        changed["claims"][0]["alternatives"][0]["branches"],
        base[0]["branches"]
    );
    assert_eq!(
        draft["statements"][1]["references"][0]["target_id"],
        "claim-0-group-0"
    );

    // Rejected: no-op, unrelated argument, sub-word/out-of-choice quote,
    // altered cue/exclusivity/branch/borrowed, or dropped group.
    assert!(apply(&selection, base.clone()).is_err());
    for bad_quote in [
        "có thể",
        "lỏng hoặc rắn",
        "nhiên liệu lỏng hoặc rắ",
        "Tên lửa có thể sử dụng nhiên liệu lỏng hoặc rắn",
    ] {
        let mut bad = base.clone();
        bad[0]["quote"] = json!(bad_quote);
        assert!(apply(&selection, bad).is_err());
    }
    let mut bad_cue = base.clone();
    bad_cue[0]["quote"] = json!("nhiên liệu lỏng hoặc rắn");
    bad_cue[0]["cue"] = json!("lỏng");
    assert!(apply(&selection, bad_cue).is_err());
    let mut bad_excl = base.clone();
    bad_excl[0]["quote"] = json!("nhiên liệu lỏng hoặc rắn");
    bad_excl[0]["exclusivity"] = json!("exclusive");
    assert!(apply(&selection, bad_excl).is_err());
    let mut bad_branch = base.clone();
    bad_branch[0]["quote"] = json!("nhiên liệu lỏng hoặc rắn");
    bad_branch[0]["branches"][0]["surface"]["quote"] = json!("lỏng");
    assert!(apply(&selection, bad_branch).is_err());
    let mut bad_borrowed = base.clone();
    bad_borrowed[0]["quote"] = json!("nhiên liệu lỏng hoặc rắn");
    bad_borrowed[0]["branches"][1]
        .as_object_mut()
        .unwrap()
        .remove("borrowed");
    assert!(apply(&selection, bad_borrowed).is_err());
    assert!(apply(&selection, json!([])).is_err());

    // Unflagged sibling group keeps its exact quote, and an unrelated bound
    // argument ("trong nhà") is excluded; only the flagged group rebinds.
    let source2 = "Máy dùng pin hoặc điện, nước hay gió trong nhà.";
    let mixed = json!({"claims":[{"subject":"Máy","predicate":"dùng","arguments":["pin hoặc điện","nước hay gió","trong nhà"],"alternatives":[
        {"quote":"dùng pin hoặc điện","cue":"hoặc","exclusivity":"unspecified","branches":[{"surface":{"quote":"pin"}},{"surface":{"quote":"điện"}}]},
        {"quote":"nước hay gió","cue":"hay","exclusivity":"unspecified","branches":[{"surface":{"quote":"nước"}},{"surface":{"quote":"gió"}}]}
    ]}]});
    let (_, v2) = assemble(source2, &mixed, &mut budget()).unwrap();
    assert_eq!(
        v2.issues,
        vec![
            "selection[0].alternatives: argument_binding",
            "selection[0].alternatives: unrepresented_choice_cue"
        ]
    );
    let choices2 = repair_choices(source2, &mixed, 0, &fields, &v2, &mut budget()).unwrap();
    assert_eq!(
        choices2["alternatives"]["quote"],
        json!(["nước hay gió", "pin hoặc điện"])
    );
    let groups = mixed["claims"][0]["alternatives"].clone();
    let mut ok = groups.clone();
    ok[0]["quote"] = json!("pin hoặc điện");
    let apply2 = |alternatives: Value| {
        apply_repair(
            source2,
            &mixed,
            0,
            &fields,
            &json!({"patch":{"alternatives":alternatives}}),
            &mut budget(),
        )
    };
    let (_, _, after2) = apply2(ok.clone()).unwrap();
    assert!(after2.issues.is_empty(), "{:?}", after2.issues);
    assert!(after2.uncovered.is_empty(), "{:?}", after2.uncovered);
    let mut swapped = ok;
    swapped[1]["quote"] = json!("pin hoặc điện");
    assert!(apply2(swapped).is_err());

    // Unrouted: under-quoted group whose bound argument has uncovered tokens,
    // sub-word mismatch, or a resolved group reference tied to the old span.
    let mut uncovered_rebind = rocket();
    uncovered_rebind["claims"][0]["alternatives"][0]["quote"] = json!("lỏng hoặc rắn");
    uncovered_rebind["claims"][0]["alternatives"][0]["branches"] =
        json!([{"surface":{"quote":"lỏng"}},{"surface":{"quote":"rắn"}}]);
    let (_, uv) = assemble(ROCKET, &uncovered_rebind, &mut budget()).unwrap();
    assert!(uv
        .issues
        .contains(&"selection[0].alternatives: argument_binding".to_owned()));
    assert!(
        argument_binding_choices(ROCKET, &uncovered_rebind, 0, &mut budget())
            .unwrap()
            .is_empty()
    );
    assert!(repair_target(ROCKET, &uncovered_rebind, &uv, &mut budget())
        .unwrap()
        .is_none());

    let mut subword = rocket();
    subword["claims"][0]["arguments"] = json!(["sử dụng nhiên liệu lỏng hoặc rắn"]);
    subword["claims"][0]["alternatives"][0]["quote"] = json!("ụng nhiên liệu lỏng hoặc rắn");
    let (_, sv) = assemble(ROCKET, &subword, &mut budget()).unwrap();
    assert!(sv
        .issues
        .contains(&"selection[0].alternatives: argument_binding".to_owned()));
    assert!(argument_binding_choices(ROCKET, &subword, 0, &mut budget())
        .unwrap()
        .is_empty());
    assert!(repair_target(ROCKET, &subword, &sv, &mut budget())
        .unwrap()
        .is_none());

    let mut ref_locked = selection.clone();
    ref_locked["claims"][1]["references"][0]["to"] =
        json!({"quote":"sử dụng nhiên liệu lỏng hoặc rắn"});
    let (_, rv) = assemble(ROCKET, &ref_locked, &mut budget()).unwrap();
    assert_eq!(
        rv.issues,
        vec![
            "selection[0].alternatives: argument_binding",
            "selection[0].alternatives: unrepresented_choice_cue",
            "selection[1].comparisons: target_unspecified"
        ]
    );
    assert!(
        argument_binding_choices(ROCKET, &ref_locked, 0, &mut budget())
            .unwrap()
            .is_empty()
    );
    assert!(repair_target(ROCKET, &ref_locked, &rv, &mut budget())
        .unwrap()
        .is_none());
}

#[test]
fn ellipsis_absent_or_ambiguous_dispatches_only_omission() {
    // D-074 (slice 1): a bound ellipsis whose `surface` or `borrowed` selector
    // fails unique resolution while its bound subject/argument quote resolves
    // uniquely in the claim scope may only be omitted; the bound role stays an
    // explicit quote and nothing is substituted or added.
    let mut selection = rocket();
    selection["claims"][1]["ellipses"] =
        json!([{"surface":{"quote":"nhiên liệu rắn"},"borrowed":{"quote":"nhiên liệu"}}]);
    let (_, validation) = assemble(ROCKET, &selection, &mut budget()).unwrap();
    assert_eq!(
        validation.issues,
        vec![
            "selection[1].comparisons: target_unspecified",
            "selection[1].ellipses: absent_or_ambiguous"
        ]
    );
    assert!(
        validation.uncovered.is_empty(),
        "{:?}",
        validation.uncovered
    );
    let (target, fields) = repair_target(ROCKET, &selection, &validation, &mut budget())
        .unwrap()
        .unwrap();
    assert_eq!((target, fields.clone()), (1, vec!["ellipses".into()]));
    let choices = repair_choices(
        ROCKET,
        &selection,
        target,
        &fields,
        &validation,
        &mut budget(),
    )
    .unwrap();
    assert_eq!(
        choices["ellipses"],
        json!({
            "count": 1,
            "surfaces": ["nhiên liệu rắn"],
            "borrowed": ["nhiên liệu"],
            "repairable": [{"ellipsis": 0, "surface": [], "may_omit": true}]
        })
    );
    let wire: Value =
        serde_json::from_str(&repair_wire_schema(ROCKET, &json!(fields), &choices).unwrap())
            .unwrap();
    let ell = &wire["properties"]["patch"]["properties"]["ellipses"];
    assert_eq!(
        (ell["minItems"].clone(), ell["maxItems"].clone()),
        (json!(0), json!(1))
    );
    let apply = |sel: &Value, ellipses: Value| {
        apply_repair(
            ROCKET,
            sel,
            target,
            &fields,
            &json!({"patch":{"ellipses":ellipses}}),
            &mut budget(),
        )
    };
    let (changed, draft, after) = apply(&selection, json!([])).unwrap();
    assert_eq!(
        after.issues,
        vec!["selection[1].comparisons: target_unspecified"]
    );
    assert!(after.uncovered.is_empty(), "{:?}", after.uncovered);
    assert_eq!(changed["claims"][1]["subject"], json!("nhiên liệu rắn"));
    assert_eq!(draft["statements"][1]["ellipses"], json!([]));
    assert_eq!(
        draft["statements"][1]["references"][0]["target_id"],
        "claim-0-group-0"
    );

    // Rejected: no-op, adding `within`, substituting borrowed/surface, extra item.
    let base = selection["claims"][1]["ellipses"].clone();
    assert!(apply(&selection, base).is_err());
    assert!(apply(
        &selection,
        json!([{"surface":{"quote":"nhiên liệu rắn"},"borrowed":{"quote":"nhiên liệu","within":"nhiên liệu lỏng"}}])
    )
    .is_err());
    assert!(apply(
        &selection,
        json!([{"surface":{"quote":"nhiên liệu rắn"},"borrowed":{"quote":"nhiên liệu lỏng"}}])
    )
    .is_err());
    assert!(apply(
        &selection,
        json!([{"surface":{"quote":"an toàn"},"borrowed":{"quote":"nhiên liệu"}}])
    )
    .is_err());

    // Invalid `surface.within` on a uniquely grounded bound argument: omission only.
    let source = "An mua táo đỏ , Bình mua xanh .";
    let ok = json!({"surface":{"quote":"xanh"},"borrowed":{"quote":"táo"}});
    let bad_within =
        json!({"surface":{"quote":"xanh","within":"An mua táo đỏ"},"borrowed":{"quote":"táo"}});
    let claims = |ellipses: Value| {
        json!({"claims":[
            {"subject":"An","predicate":"mua","arguments":["táo đỏ"],"anchor":"An mua táo đỏ"},
            {"subject":"Bình","predicate":"mua","arguments":["xanh"],"anchor":"Bình mua xanh","ellipses":ellipses}
        ]})
    };
    let bad_surface = claims(json!([bad_within.clone()]));
    let (_, bsv) = assemble(source, &bad_surface, &mut budget()).unwrap();
    assert_eq!(
        bsv.issues,
        vec!["selection[1].ellipses: absent_or_ambiguous"]
    );
    assert!(apply_repair(
        source,
        &bad_surface,
        1,
        &fields,
        &json!({"patch":{"ellipses":[]}}),
        &mut budget(),
    )
    .is_ok());
    // Dropping `surface.within` to keep the ellipsis is rejected; only omission is allowed.
    assert!(apply_repair(
        source,
        &bad_surface,
        1,
        &fields,
        &json!({"patch":{"ellipses":[ok.clone()]}}),
        &mut budget(),
    )
    .is_err());

    // Unflagged sibling stays byte-identical: only the unresolved ellipsis may go.
    let bad_borrowed = json!({"surface":{"quote":"Bình"},"borrowed":{"quote":"mua"}});
    let mixed = claims(json!([ok.clone(), bad_borrowed.clone()]));
    let (_, mv) = assemble(source, &mixed, &mut budget()).unwrap();
    assert_eq!(
        mv.issues,
        vec!["selection[1].ellipses: absent_or_ambiguous"]
    );
    assert!(apply_repair(
        source,
        &mixed,
        1,
        &fields,
        &json!({"patch":{"ellipses":[ok]}}),
        &mut budget(),
    )
    .is_ok());
    assert!(apply_repair(
        source,
        &mixed,
        1,
        &fields,
        &json!({"patch":{"ellipses":[bad_borrowed]}}),
        &mut budget(),
    )
    .is_err());
    assert!(apply_repair(
        source,
        &mixed,
        1,
        &fields,
        &json!({"patch":{"ellipses":[]}}),
        &mut budget(),
    )
    .is_err());

    // Combined `role_binding` + unresolved `borrowed`: no futile surface rebind
    // is offered when `borrowed` itself fails unique resolution; omission clears both.
    let combined = claims(json!([{"surface":{"quote":"mua xanh"},"borrowed":{"quote":"mua"}}]));
    let (_, cv) = assemble(source, &combined, &mut budget()).unwrap();
    assert_eq!(
        cv.issues,
        vec![
            "selection[1].ellipses: absent_or_ambiguous",
            "selection[1].ellipses: role_binding"
        ]
    );
    let cc = repair_choices(source, &combined, 1, &fields, &cv, &mut budget()).unwrap();
    assert_eq!(
        cc["ellipses"]["repairable"],
        json!([{"ellipsis": 0, "surface": [], "may_omit": true}])
    );
    assert!(apply_repair(
        source,
        &combined,
        1,
        &fields,
        &json!({"patch":{"ellipses":[]}}),
        &mut budget(),
    )
    .is_ok());
    assert!(apply_repair(
        source,
        &combined,
        1,
        &fields,
        &json!({"patch":{"ellipses":[{"surface":{"quote":"xanh"},"borrowed":{"quote":"mua"}}]}}),
        &mut budget(),
    )
    .is_err());
}

#[test]
fn references_unresolved_target_dispatches_only_local_token_run_narrowing() {
    // D-074 (slice 2): a reference flagged `target_absent_ambiguous_or_self`
    // may only narrow `to.quote` to a contiguous whitespace-bounded token run
    // of the emitted `to.quote` that resolves uniquely (any emitted `to.within`
    // kept) without overlapping `via` to one addressable target of the same
    // `target_kind` in another claim without creating a `cycle`.
    let mut selection = rocket();
    selection["claims"][1]["references"][0]["to"] =
        json!({"quote":"sử dụng nhiên liệu lỏng hoặc rắn"});
    let (_, validation) = assemble(ROCKET, &selection, &mut budget()).unwrap();
    assert_eq!(
        validation.issues,
        vec![
            "selection[1].comparisons: target_unspecified",
            "selection[1].references: target_absent_ambiguous_or_self"
        ]
    );
    assert!(
        validation.uncovered.is_empty(),
        "{:?}",
        validation.uncovered
    );
    let (target, fields) = repair_target(ROCKET, &selection, &validation, &mut budget())
        .unwrap()
        .unwrap();
    assert_eq!((target, fields.clone()), (1, vec!["references".into()]));
    let choices = repair_choices(
        ROCKET,
        &selection,
        target,
        &fields,
        &validation,
        &mut budget(),
    )
    .unwrap();
    // Only the exact `group` span of claim 0 matches `target_kind: "group"`.
    assert_eq!(
        choices["references"],
        json!({
            "count": 1,
            "via": ["Trong đó"],
            "to": ["nhiên liệu lỏng hoặc rắn"],
            "target_kind": ["group"],
            "repairable": [{"reference": 0, "to": ["nhiên liệu lỏng hoặc rắn"]}]
        })
    );
    let wire: Value =
        serde_json::from_str(&repair_wire_schema(ROCKET, &json!(fields), &choices).unwrap())
            .unwrap();
    let refs = &wire["properties"]["patch"]["properties"]["references"];
    assert_eq!(
        (refs["minItems"].clone(), refs["maxItems"].clone()),
        (json!(1), json!(1))
    );
    assert_eq!(
        refs["items"]["properties"]["to"]["properties"]["quote"]["enum"],
        json!(["nhiên liệu lỏng hoặc rắn"])
    );
    assert_eq!(
        refs["items"]["properties"]["via"]["properties"]["quote"]["enum"],
        json!(["Trong đó"])
    );
    let apply = |sel: &Value, references: Value| {
        apply_repair(
            ROCKET,
            sel,
            target,
            &fields,
            &json!({"patch":{"references":references}}),
            &mut budget(),
        )
    };
    let base = selection["claims"][1]["references"].clone();
    let mut narrowed = base.clone();
    narrowed[0]["to"]["quote"] = json!("nhiên liệu lỏng hoặc rắn");
    let (changed, draft, after) = apply(&selection, narrowed).unwrap();
    assert_eq!(
        after.issues,
        vec!["selection[1].comparisons: target_unspecified"]
    );
    assert!(after.uncovered.is_empty(), "{:?}", after.uncovered);
    assert_eq!(changed["claims"][0], selection["claims"][0]);
    assert_eq!(
        draft["statements"][1]["references"][0]["target_id"],
        "claim-0-group-0"
    );

    // Rejected: no-op, term/sub-word/out-of-choice `to.quote`, altered `via` or
    // `target_kind`, added `to.within`, or dropped reference.
    assert!(apply(&selection, base.clone()).is_err());
    for bad in [
        "nhiên liệu lỏng",
        "rắn",
        "nhiên liệu lỏng hoặc rắ",
        "Tên lửa",
    ] {
        let mut r = base.clone();
        r[0]["to"]["quote"] = json!(bad);
        assert!(apply(&selection, r).is_err());
    }
    let mut revia = base.clone();
    revia[0]["to"]["quote"] = json!("nhiên liệu lỏng hoặc rắn");
    revia[0]["via"] = json!({"quote":"sẽ"});
    assert!(apply(&selection, revia).is_err());
    let mut rekind = base.clone();
    rekind[0]["to"]["quote"] = json!("nhiên liệu lỏng hoặc rắn");
    rekind[0]["target_kind"] = json!("term");
    assert!(apply(&selection, rekind).is_err());
    let mut scoped = base.clone();
    scoped[0]["to"] = json!({
        "quote": "nhiên liệu lỏng hoặc rắn",
        "within": "Tên lửa có thể sử dụng nhiên liệu lỏng hoặc rắn"
    });
    assert!(apply(&selection, scoped).is_err());
    assert!(apply(&selection, json!([])).is_err());

    // Emitted `to.within` is preserved: keeping it succeeds, dropping it fails.
    let mut within = rocket();
    within["claims"][1]["references"][0]["to"] = json!({
        "quote": "sử dụng nhiên liệu lỏng hoặc rắn",
        "within": "Tên lửa có thể sử dụng nhiên liệu lỏng hoặc rắn"
    });
    let (_, wv) = assemble(ROCKET, &within, &mut budget()).unwrap();
    assert!(wv
        .issues
        .contains(&"selection[1].references: target_absent_ambiguous_or_self".to_owned()));
    let mut kept = within["claims"][1]["references"].clone();
    kept[0]["to"]["quote"] = json!("nhiên liệu lỏng hoặc rắn");
    assert!(apply(&within, kept).is_ok());
    let mut dropped = within["claims"][1]["references"].clone();
    dropped[0]["to"] = json!({"quote":"nhiên liệu lỏng hoặc rắn"});
    assert!(apply(&within, dropped).is_err());

    // Unflagged sibling reference keeps its exact `to`; only the flagged one narrows.
    let mut mixed = rocket();
    mixed["claims"][1]["references"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "via": {"quote": "sẽ"},
            "to": {"quote": "chiếc Tên lửa"},
            "target_kind": "term"
        }));
    let (_, mv) = assemble(ROCKET, &mixed, &mut budget()).unwrap();
    assert!(mv
        .issues
        .contains(&"selection[1].references: target_absent_ambiguous_or_self".to_owned()));
    let mc = repair_choices(ROCKET, &mixed, 1, &fields, &mv, &mut budget()).unwrap();
    assert_eq!(
        mc["references"]["to"],
        json!(["Tên lửa", "nhiên liệu lỏng hoặc rắn"])
    );
    let refs = mixed["claims"][1]["references"].clone();
    let mut ok = refs.clone();
    ok[1]["to"]["quote"] = json!("Tên lửa");
    let (_, mdraft, _) = apply(&mixed, ok.clone()).unwrap();
    assert_eq!(
        mdraft["statements"][1]["references"][1]["target_id"],
        "claim-0-role-0"
    );
    let mut swapped = ok;
    swapped[0]["to"]["quote"] = json!("Tên lửa");
    assert!(apply(&mixed, swapped).is_err());

    // Unrouted: self-target only, cycle-inducing target, or `via`-overlapping target.
    let self_only = json!({"claims":[
        {"subject":"A","predicate":"chạy","anchor":"A chạy"},
        {"subject":"B","predicate":"dừng","anchor":"B dừng","references":[{"via":{"quote":"dừng"},"to":{"quote":"lúc B"},"target_kind":"term"}]}
    ]});
    let (_, sv) = assemble("A chạy. B dừng.", &self_only, &mut budget()).unwrap();
    assert!(sv
        .issues
        .contains(&"selection[1].references: target_absent_ambiguous_or_self".to_owned()));
    assert!(
        reference_target_choices("A chạy. B dừng.", &self_only, 1, &mut budget())
            .unwrap()
            .is_empty()
    );
    assert!(
        repair_target("A chạy. B dừng.", &self_only, &sv, &mut budget())
            .unwrap()
            .is_none()
    );

    let cyclic = json!({"claims":[
        {"subject":"A","predicate":"chạy","anchor":"A chạy","references":[{"via":{"quote":"chạy"},"to":{"quote":"B"},"target_kind":"term"}]},
        {"subject":"B","predicate":"dừng","anchor":"B dừng","references":[{"via":{"quote":"dừng"},"to":{"quote":"ông A"},"target_kind":"term"}]}
    ]});
    let (_, cyv) = assemble("A chạy. B dừng.", &cyclic, &mut budget()).unwrap();
    assert!(cyv
        .issues
        .contains(&"selection[1].references: target_absent_ambiguous_or_self".to_owned()));
    assert!(
        reference_target_choices("A chạy. B dừng.", &cyclic, 1, &mut budget())
            .unwrap()
            .is_empty()
    );
    assert!(
        repair_target("A chạy. B dừng.", &cyclic, &cyv, &mut budget())
            .unwrap()
            .is_none()
    );
}

#[test]
fn alternatives_group_ambiguous_dispatches_only_bound_argument_rebind() {
    // D-074 (slice 3): an OR group flagged `group_ambiguous` (`group.quote`
    // fails unique resolution in the claim scope) with `exclusivity:
    // unspecified` and no incompatible cue may only rebind `group.quote` to a
    // bound `argument` of the same claim that is a whitespace-bounded token
    // extension and resolves `cue`, ordered disjoint `branches` and `borrowed`
    // cleanly with no new issue or coverage loss.
    let mut selection = rocket();
    selection["claims"][0]["alternatives"][0]["quote"] = json!("loại nhiên liệu lỏng hoặc rắn");
    let (_, validation) = assemble(ROCKET, &selection, &mut budget()).unwrap();
    assert_eq!(
        validation.issues,
        vec![
            "selection[0].alternatives: group_ambiguous",
            "selection[0].alternatives: unrepresented_choice_cue",
            "selection[1].comparisons: target_unspecified",
            "selection[1].references: target_absent_ambiguous_or_self"
        ]
    );
    assert!(
        validation.uncovered.is_empty(),
        "{:?}",
        validation.uncovered
    );
    let (target, fields) = repair_target(ROCKET, &selection, &validation, &mut budget())
        .unwrap()
        .unwrap();
    assert_eq!((target, fields.clone()), (0, vec!["alternatives".into()]));
    let choices = repair_choices(
        ROCKET,
        &selection,
        target,
        &fields,
        &validation,
        &mut budget(),
    )
    .unwrap();
    assert_eq!(
        choices["alternatives"],
        json!({
            "groups": 1,
            "repairable": [],
            "borrowed": ["nhiên liệu"],
            "argument_binding": [{"group": 0, "quote": ["nhiên liệu lỏng hoặc rắn"]}],
            "quote": ["nhiên liệu lỏng hoặc rắn"]
        })
    );
    let wire: Value =
        serde_json::from_str(&repair_wire_schema(ROCKET, &json!(fields), &choices).unwrap())
            .unwrap();
    assert_eq!(
        wire["properties"]["patch"]["properties"]["alternatives"]["items"]["properties"]["quote"]
            ["enum"],
        json!(["nhiên liệu lỏng hoặc rắn"])
    );
    let apply = |sel: &Value, alternatives: Value| {
        apply_repair(
            ROCKET,
            sel,
            target,
            &fields,
            &json!({"patch":{"alternatives":alternatives}}),
            &mut budget(),
        )
    };
    let base = selection["claims"][0]["alternatives"].clone();
    let mut rebound = base.clone();
    rebound[0]["quote"] = json!("nhiên liệu lỏng hoặc rắn");
    let (changed, draft, after) = apply(&selection, rebound).unwrap();
    assert_eq!(
        after.issues,
        vec!["selection[1].comparisons: target_unspecified"]
    );
    assert!(after.uncovered.is_empty(), "{:?}", after.uncovered);
    assert_eq!(
        changed["claims"][0]["alternatives"][0]["branches"],
        base[0]["branches"]
    );
    assert_eq!(
        draft["statements"][1]["references"][0]["target_id"],
        "claim-0-group-0"
    );

    // Rejected: no-op, unrelated argument, sub-word/out-of-choice quote,
    // altered cue/exclusivity/branch/borrowed, or dropped group.
    assert!(apply(&selection, base.clone()).is_err());
    for bad_quote in ["có thể", "lỏng hoặc rắn", "nhiên liệu lỏng hoặc rắ"] {
        let mut bad = base.clone();
        bad[0]["quote"] = json!(bad_quote);
        assert!(apply(&selection, bad).is_err());
    }
    let mut bad_excl = base.clone();
    bad_excl[0]["quote"] = json!("nhiên liệu lỏng hoặc rắn");
    bad_excl[0]["exclusivity"] = json!("exclusive");
    assert!(apply(&selection, bad_excl).is_err());
    assert!(apply(&selection, json!([])).is_err());

    // Ambiguous repeated shorter `group.quote` in scope rebinds up to the
    // unique bound argument when `cue` + `branches` cover it cleanly.
    let rep_source = "An dùng pin hay điện. Bình kiểm tra pin hoặc điện mới.";
    let rep_sel = json!({"claims":[
        {"subject":"An","predicate":"dùng","arguments":["pin hay điện"]},
        {"subject":"Bình","predicate":"kiểm tra","arguments":["pin hoặc điện mới"],"alternatives":[
            {"quote":"pin","cue":"hoặc","exclusivity":"unspecified","branches":[
                {"surface":{"quote":"pin","within":"pin hoặc điện mới"}},
                {"surface":{"quote":"điện mới"}}
            ]}
        ]}
    ]});
    let (_, rv) = assemble(rep_source, &rep_sel, &mut budget()).unwrap();
    assert_eq!(
        rv.issues,
        vec![
            "selection[1].alternatives: group_ambiguous",
            "selection[1].alternatives: unrepresented_choice_cue"
        ]
    );
    let mut rep_ok = rep_sel["claims"][1]["alternatives"].clone();
    rep_ok[0]["quote"] = json!("pin hoặc điện mới");
    let (_, _, rafter) = apply_repair(
        rep_source,
        &rep_sel,
        1,
        &fields,
        &json!({"patch":{"alternatives":rep_ok}}),
        &mut budget(),
    )
    .unwrap();
    assert!(rafter.issues.is_empty(), "{:?}", rafter.issues);
    assert!(rafter.uncovered.is_empty(), "{:?}", rafter.uncovered);

    // Unrouted: non-default exclusivity on an unresolved group, uncovered
    // bound-argument tokens, or sub-word mismatch.
    let mut non_default_excl = selection.clone();
    non_default_excl["claims"][0]["alternatives"][0]["exclusivity"] = json!("exclusive");
    assert!(
        argument_binding_choices(ROCKET, &non_default_excl, 0, &mut budget())
            .unwrap()
            .is_empty()
    );
    let mut uncovered = selection.clone();
    uncovered["claims"][0]["alternatives"][0]["branches"] =
        json!([{"surface":{"quote":"lỏng"}},{"surface":{"quote":"rắn"}}]);
    assert!(
        argument_binding_choices(ROCKET, &uncovered, 0, &mut budget())
            .unwrap()
            .is_empty()
    );
    let mut subword = rocket();
    subword["claims"][0]["alternatives"][0]["quote"] = json!("hiên liệu lỏng hoặc rắn");
    assert!(argument_binding_choices(ROCKET, &subword, 0, &mut budget())
        .unwrap()
        .is_empty());
}

#[test]
fn incompatible_or_cue_in_mixed_quote_dispatches_only_between_branch_or_recue() {
    // D-074 (slice 4): a bound OR group flagged `incompatible_or_cue` whose
    // `group.quote` also carries a lexical OR cue (`hoặc`/`or`) and whose
    // ordered disjoint `branches` and `borrowed` reconstructions resolve
    // cleanly may only re-cue `group.cue` to a whitespace-bounded lexical OR
    // token of `group.quote` resolving uniquely strictly between adjacent
    // branches with no new issue or coverage loss; whole-group omission stays
    // forbidden because `group.quote` contains an OR cue.
    let source = "Hệ thống hỗ trợ pin và cáp hoặc điện dự phòng.";
    let selection = json!({"claims":[{
        "subject": "Hệ thống",
        "predicate": "hỗ trợ",
        "arguments": ["pin và cáp hoặc điện dự phòng"],
        "alternatives": [{
            "quote": "pin và cáp hoặc điện dự phòng",
            "cue": "và",
            "exclusivity": "unspecified",
            "branches": [
                {"surface": {"quote": "pin và cáp"}},
                {"surface": {"quote": "điện dự phòng"}}
            ]
        }]
    }]});
    let (_, validation) = assemble(source, &selection, &mut budget()).unwrap();
    assert_eq!(
        validation.issues,
        vec!["selection[0].alternatives: incompatible_or_cue"]
    );
    assert_eq!(validation.uncovered, vec!["hoặc"]);
    let (target, fields) = repair_target(source, &selection, &validation, &mut budget())
        .unwrap()
        .unwrap();
    assert_eq!((target, fields.clone()), (0, vec!["alternatives".into()]));
    let choices = repair_choices(
        source,
        &selection,
        target,
        &fields,
        &validation,
        &mut budget(),
    )
    .unwrap();
    assert_eq!(
        choices["alternatives"],
        json!({
            "groups": 1,
            "repairable": [],
            "borrowed": [],
            "cue_narrowing": [{"group": 0, "cue": ["hoặc"]}],
            "cue": ["hoặc"]
        })
    );
    let wire: Value =
        serde_json::from_str(&repair_wire_schema(source, &json!(fields), &choices).unwrap())
            .unwrap();
    let alt = &wire["properties"]["patch"]["properties"]["alternatives"];
    assert_eq!(
        (alt["minItems"].clone(), alt["maxItems"].clone()),
        (json!(1), json!(1))
    );
    assert_eq!(alt["items"]["properties"]["cue"]["enum"], json!(["hoặc"]));
    let apply = |sel: &Value, alternatives: Value| {
        apply_repair(
            source,
            sel,
            target,
            &fields,
            &json!({"patch":{"alternatives":alternatives}}),
            &mut budget(),
        )
    };
    let base = selection["claims"][0]["alternatives"].clone();
    let mut recued = base.clone();
    recued[0]["cue"] = json!("hoặc");
    let (changed, draft, after) = apply(&selection, recued).unwrap();
    assert!(after.issues.is_empty(), "{:?}", after.issues);
    assert!(after.uncovered.is_empty(), "{:?}", after.uncovered);
    assert_eq!(
        changed["claims"][0]["alternatives"][0]["branches"],
        base[0]["branches"]
    );
    assert!(draft["statements"][0]["alternatives"][0]["cue_span"].is_array());

    // Rejected: no-op, whole-group drop (would raise `unrepresented_choice_cue`),
    // non-OR token, altered quote/exclusivity/branches.
    assert!(apply(&selection, base.clone()).is_err());
    assert!(apply(&selection, json!([])).is_err());
    for bad_cue in ["cáp", "or", "điện"] {
        let mut bad = base.clone();
        bad[0]["cue"] = json!(bad_cue);
        assert!(apply(&selection, bad).is_err());
    }
    let mut bad_excl = base.clone();
    bad_excl[0]["cue"] = json!("hoặc");
    bad_excl[0]["exclusivity"] = json!("exclusive");
    assert!(apply(&selection, bad_excl).is_err());
    let mut bad_branch = base.clone();
    bad_branch[0]["cue"] = json!("hoặc");
    bad_branch[0]["branches"][0]["surface"]["quote"] = json!("pin");
    assert!(apply(&selection, bad_branch).is_err());

    // English mixed quote (`and` -> `or`) also preserves unflagged sibling cue.
    let en_source = "The device supports mouse and keyboard or touch, plus wifi or wired.";
    let en_sel = json!({"claims":[{
        "subject": "The device",
        "predicate": "supports",
        "arguments": ["mouse and keyboard or touch", "plus wifi or wired"],
        "alternatives": [
            {
                "quote": "mouse and keyboard or touch",
                "cue": "and",
                "exclusivity": "unspecified",
                "branches": [
                    {"surface": {"quote": "mouse and keyboard"}},
                    {"surface": {"quote": "touch"}}
                ]
            },
            {
                "quote": "plus wifi or wired",
                "cue": "or",
                "exclusivity": "unspecified",
                "branches": [
                    {"surface": {"quote": "plus wifi"}},
                    {"surface": {"quote": "wired"}}
                ]
            }
        ]
    }]});
    let (_, en_v) = assemble(en_source, &en_sel, &mut budget()).unwrap();
    assert_eq!(
        en_v.issues,
        vec!["selection[0].alternatives: incompatible_or_cue"]
    );
    let mut en_ok = en_sel["claims"][0]["alternatives"].clone();
    en_ok[0]["cue"] = json!("or");
    let (_, _, en_after) = apply_repair(
        en_source,
        &en_sel,
        0,
        &fields,
        &json!({"patch":{"alternatives":en_ok}}),
        &mut budget(),
    )
    .unwrap();
    assert!(en_after.issues.is_empty(), "{:?}", en_after.issues);
    assert!(en_after.uncovered.is_empty(), "{:?}", en_after.uncovered);

    // Unrouted: branches split on `và` (OR token lies outside/inside branches),
    // uncovered `và` when re-cued, or repeated `hoặc` inside `group.quote`.
    for (src, sel) in [
        (
            "Hộp chứa bi xanh và bi đỏ hoặc bi vàng.",
            json!({"claims":[{"subject":"Hộp","predicate":"chứa","arguments":["bi xanh và bi đỏ hoặc bi vàng"],"alternatives":[{"quote":"bi xanh và bi đỏ hoặc bi vàng","cue":"và","exclusivity":"unspecified","branches":[{"surface":{"quote":"bi xanh"}},{"surface":{"quote":"bi đỏ"}}]}]}]}),
        ),
        (
            "Hệ thống hỗ trợ pin và cáp hoặc điện dự phòng.",
            json!({"claims":[{"subject":"Hệ thống","predicate":"hỗ trợ","arguments":["pin và cáp hoặc điện dự phòng"],"alternatives":[{"quote":"pin và cáp hoặc điện dự phòng","cue":"và","exclusivity":"unspecified","branches":[{"surface":{"quote":"pin"}},{"surface":{"quote":"điện dự phòng"}}]}]}]}),
        ),
        (
            "Hệ thống hỗ trợ pin và cáp hoặc điện hoặc gió.",
            json!({"claims":[{"subject":"Hệ thống","predicate":"hỗ trợ","arguments":["pin và cáp hoặc điện hoặc gió"],"alternatives":[{"quote":"pin và cáp hoặc điện hoặc gió","cue":"và","exclusivity":"unspecified","branches":[{"surface":{"quote":"pin và cáp"}},{"surface":{"quote":"điện"}},{"surface":{"quote":"gió"}}]}]}]}),
        ),
    ] {
        let (_, v) = assemble(src, &sel, &mut budget()).unwrap();
        assert!(v
            .issues
            .contains(&"selection[0].alternatives: incompatible_or_cue".to_owned()));
        assert!(alternative_cue_choices(src, &sel, 0, &mut budget())
            .unwrap()
            .is_empty());
        assert!(repair_target(src, &sel, &v, &mut budget())
            .unwrap()
            .is_none());
    }
}
