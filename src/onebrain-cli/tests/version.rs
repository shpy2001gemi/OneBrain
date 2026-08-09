use std::process::Command;

#[test]
fn verbose_version_and_base_status_do_not_start_a_node() {
    let temp = tempfile::tempdir().unwrap();
    for arguments in [
        ["--version", "--verbose"].as_slice(),
        ["base", "status"].as_slice(),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_onebrain"))
            .args(arguments)
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["base_version"]["major"], 1);
        assert_eq!(value["archive_profile"]["major"], 2);
        assert_eq!(value["archive_profile"]["minor"], 0);
        assert_eq!(value["qualification"]["state"], "unqualified");
        assert_eq!(value["features"]["base_v1"], true);
        assert_eq!(value["features"]["distributed_requested"], false);
        assert_eq!(value["features"]["distributed_active"], false);
        assert_eq!(
            value["candidate_semantic_digest"].as_str().unwrap().len(),
            64
        );
        assert_eq!(value["artifact_tuple_digest"].as_str().unwrap().len(), 64);
    }
    assert!(!temp.path().join("onebrain_data").exists());
}

#[test]
fn short_version_is_stable_plain_text() {
    let output = Command::new(env!("CARGO_BIN_EXE_onebrain"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .starts_with("onebrain "));
}
