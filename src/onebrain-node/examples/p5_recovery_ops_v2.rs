//! Source-free smoke wrapper for the closed P5 V2 recovery library.

use onebrain_node::vnext_p5_recovery_ops_v2::{
    explicit_re_enable, obarv002_restore, rollback, verify_inputs, P5RecoveryInputsV2,
    P5RecoveryOperationV2,
};
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("P5 V2 recovery failed: {error:?}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), onebrain_node::vnext_p5_recovery_ops_v2::P5RecoveryErrorV2> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() < 13
        || args[1] != "--request-digest"
        || args[3] != "--session-id"
        || args[5] != "--host-id"
        || args[7] != "--operation-id"
        || args[9] != "--runner-data-root"
        || args[11] != "--evidence-output"
    {
        return Err(onebrain_node::vnext_p5_recovery_ops_v2::P5RecoveryErrorV2::UnexpectedInput);
    }
    let operation = match args[0].as_str() {
        "obarv002-restore" => P5RecoveryOperationV2::Obarv002Restore,
        "rollback" => P5RecoveryOperationV2::Rollback,
        "explicit-re-enable" => P5RecoveryOperationV2::ExplicitReEnable,
        _ => {
            return Err(onebrain_node::vnext_p5_recovery_ops_v2::P5RecoveryErrorV2::UnexpectedInput)
        }
    };
    let mut input = P5RecoveryInputsV2 {
        request_digest: parse32(&args[2])?,
        session_id: parse32(&args[4])?,
        host_id: args[6].clone(),
        operation_id: parse32(&args[8])?,
        runner_data_root: PathBuf::from(&args[10]),
        evidence_output: PathBuf::from(&args[12]),
        archive_input: None,
        previous_generation: None,
    };
    if args.len() == 15 {
        match (operation, args[13].as_str()) {
            (P5RecoveryOperationV2::Obarv002Restore, "--archive-input") => {
                input.archive_input = Some(PathBuf::from(&args[14]))
            }
            (P5RecoveryOperationV2::Rollback, "--previous-generation") => {
                input.previous_generation = Some(PathBuf::from(&args[14]))
            }
            _ => {
                return Err(
                    onebrain_node::vnext_p5_recovery_ops_v2::P5RecoveryErrorV2::UnexpectedInput,
                )
            }
        }
    } else if args.len() != 13 {
        return Err(onebrain_node::vnext_p5_recovery_ops_v2::P5RecoveryErrorV2::UnexpectedInput);
    }
    let verified = verify_inputs(operation, &input)?;
    let receipt = match operation {
        P5RecoveryOperationV2::Obarv002Restore => obarv002_restore(verified)?,
        P5RecoveryOperationV2::Rollback => rollback(verified)?,
        P5RecoveryOperationV2::ExplicitReEnable => explicit_re_enable(verified)?,
    };
    println!("{}", hex(&receipt.evidence_blake3));
    Ok(())
}

fn parse32(
    value: &str,
) -> Result<[u8; 32], onebrain_node::vnext_p5_recovery_ops_v2::P5RecoveryErrorV2> {
    if value.len() != 64 {
        return Err(onebrain_node::vnext_p5_recovery_ops_v2::P5RecoveryErrorV2::EmptyBinding);
    }
    let mut out = [0; 32];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).map_err(|_| {
            onebrain_node::vnext_p5_recovery_ops_v2::P5RecoveryErrorV2::EmptyBinding
        })?;
    }
    Ok(out)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
