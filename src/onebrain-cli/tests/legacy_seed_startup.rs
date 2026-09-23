use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn default_start_does_not_attempt_legacy_seed_discovery() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_onebrain"))
        .args([
            "start",
            "--port",
            "0",
            "--data-dir",
            directory.path().to_str().unwrap(),
            "--concept-registry-mode",
            "disabled",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("default start did not finish before the bounded deadline");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Legacy seed compatibility: disabled"));
    assert!(!stdout.contains("Trying seed:"));
    assert!(stdout.contains("Goodbye!"));
}
