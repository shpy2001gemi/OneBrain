use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use onebrain_node::run_p5_canary_preflight;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let mut output = PathBuf::from("target/p5-01/canary-preflight-report.json");
    let mut data_dir = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--output" => {
                output = PathBuf::from(args.next().unwrap_or_else(|| {
                    eprintln!("--output requires a path");
                    std::process::exit(2);
                }));
            }
            "--data-dir" => {
                data_dir = Some(PathBuf::from(args.next().unwrap_or_else(|| {
                    eprintln!("--data-dir requires a path");
                    std::process::exit(2);
                })));
            }
            _ => {
                eprintln!("unknown argument: {argument}");
                std::process::exit(2);
            }
        }
    }
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let data_dir = data_dir.unwrap_or_else(|| {
        std::env::temp_dir().join(format!("onebrain-p5-01-{}-{epoch}", std::process::id()))
    });
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|error| {
            eprintln!("cannot create P5-01 report directory: {error}");
            std::process::exit(2);
        });
    }
    let report = run_p5_canary_preflight(&data_dir)
        .await
        .unwrap_or_else(|error| {
            eprintln!("{error}");
            std::process::exit(1);
        });
    let json = serde_json::to_vec_pretty(&report).unwrap_or_else(|error| {
        eprintln!("cannot serialize P5-01 report: {error}");
        std::process::exit(1);
    });
    let mut file = File::create(&output).unwrap_or_else(|error| {
        eprintln!("cannot create P5-01 report: {error}");
        std::process::exit(1);
    });
    file.write_all(&json)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .unwrap_or_else(|error| {
            eprintln!("cannot persist P5-01 report: {error}");
            std::process::exit(1);
        });
    println!("{}", String::from_utf8_lossy(&json));
    if !report.passes() {
        std::process::exit(1);
    }
}
