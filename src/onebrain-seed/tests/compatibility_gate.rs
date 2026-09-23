use std::process::Command;

#[test]
fn legacy_daemon_refuses_to_bind_without_explicit_compatibility_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_onebrain-seed"))
        .args(["--port", "0"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--legacy-seed-compat"));
    assert!(output.stdout.is_empty());
}
