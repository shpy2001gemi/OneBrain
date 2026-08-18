use std::fs;
use std::path::{Path, PathBuf};

use onebrain_node::vnext_fuzz_targets::{
    run_target, valid_reachability_corpus_seed, FUZZ_TARGETS, MAX_FUZZ_INPUT_BYTES,
    REACHABILITY_INVALID_CORPUS,
};
use sha2::{Digest, Sha256};

const SEEDS_PER_TARGET: usize = 3;
const CORPUS_MANIFEST_SHA256: &str =
    "578d5abef5cfe9cc57eea95e77c2f4c3b0faa04cde0b688a973fc3ebaa91f7a2";

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fuzz/corpus");
    let mut digest = Sha256::new();
    digest.update(b"onebrain:dr-m5:fuzz-corpus:1\0");
    let mut case_count = 0usize;

    for target in FUZZ_TARGETS {
        update_field(&mut digest, target.as_bytes());
        let directory = root.join(target);
        let files = corpus_files(&directory);
        assert_eq!(
            files.len(),
            SEEDS_PER_TARGET,
            "{target} must have exactly {SEEDS_PER_TARGET} frozen PR seeds"
        );
        for path in files {
            let name = path
                .file_name()
                .expect("corpus path has a file name")
                .to_string_lossy();
            let data = fs::read(&path).expect("frozen corpus seed is readable");
            assert!(
                !data.is_empty() && data.len() <= MAX_FUZZ_INPUT_BYTES,
                "{} must be non-empty and bounded",
                path.display()
            );
            update_field(&mut digest, name.as_bytes());
            update_field(&mut digest, &data);
            run_target(target, &data);
            case_count += 1;
        }
    }

    for (class, data) in REACHABILITY_INVALID_CORPUS {
        update_field(&mut digest, class.as_bytes());
        update_field(&mut digest, data);
        run_target("reachability_codec", data);
        case_count += 1;
    }
    let valid_reachability = valid_reachability_corpus_seed();
    update_field(&mut digest, b"valid-reachability-object");
    update_field(&mut digest, &valid_reachability);
    run_target("reachability_codec", &valid_reachability);
    case_count += 1;

    let observed = format!("{:x}", digest.finalize());
    assert_eq!(observed, CORPUS_MANIFEST_SHA256);
    let oracle =
        ku_net::vnext_chaos::expected_oracle_root(64).expect("frozen trace record count is valid");
    println!("DR_M5_FUZZ_CORPUS_CASES={case_count}");
    println!("DR_M5_FUZZ_CORPUS_SHA256={observed}");
    println!("DR_M5_CHAOS_ORACLE_BLAKE3={}", hex(&oracle));
}

fn corpus_files(directory: &Path) -> Vec<PathBuf> {
    let mut files = fs::read_dir(directory)
        .expect("frozen target corpus directory exists")
        .map(|entry| entry.expect("corpus directory entry is readable").path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn update_field(digest: &mut Sha256, bytes: &[u8]) {
    digest.update((bytes.len() as u64).to_be_bytes());
    digest.update(bytes);
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
