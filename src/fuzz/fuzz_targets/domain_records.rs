#![no_main]

use libfuzzer_sys::fuzz_target;
use onebrain_node::vnext_fuzz_targets::{run_target, MAX_FUZZ_INPUT_BYTES};

fuzz_target!(|data: &[u8]| {
    if data.len() <= MAX_FUZZ_INPUT_BYTES {
        run_target("domain_records", data);
    }
});
