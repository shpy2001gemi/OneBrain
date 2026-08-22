use std::io::Read;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ed25519_dalek::SigningKey;
use ku_net::vnext_reachability_crypto::{
    InMemoryReachabilityReplayStore, ReachabilityAdmission, ReachabilityAdmissionPreparer,
    ReachabilityDialValidator, ReachabilityLockFreeDialValidation, ReachabilityLockFreePreparation,
    ReachabilityRecordAdmission, SystemPublicEndpointResolver,
};
use ku_net::vnext_relay_tunnel::prove_relay_possession;
use onebrain_protocol::{encode_relay_control, RelayControlV1, RelayTransportV1};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

const MAX_REQUEST_BYTES: u64 = 65_536;
const PROBE_DEADLINE: Duration = Duration::from_secs(20);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeRequestV1 {
    format: u64,
    kind: String,
    source_host_id: String,
    relay_host_id: String,
    candidate_descriptor_hex: String,
    verifier_context: String,
}

#[derive(Serialize)]
struct ProbeReceiptV2 {
    descriptor_blake3: String,
    format: u64,
    probes: Vec<EndpointProbeV2>,
    relay_descriptor_hex: String,
    relay_host_id: String,
    relay_node_id: String,
    source_host_id: String,
}

#[derive(Serialize)]
struct EndpointProbeV2 {
    endpoint_index: usize,
    possession_proof_blake3: String,
    success: bool,
    transcript_blake3: String,
    transport: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("OBP_RELAY_PREFLIGHT: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments == ["--print-compiled-binding"] {
        let value = serde_json::json!({
            "candidate_commit": option_env!("ONEBRAIN_BASE_COMMIT").unwrap_or("unbound"),
            "candidate_tree": option_env!("ONEBRAIN_SOURCE_TREE").unwrap_or("bound-by-bundle-provenance"),
            "format": "onebrain/relay-preflight-compiled-binding/1",
            "toolchain_digest": option_env!("ONEBRAIN_TOOLCHAIN_DIGEST").unwrap_or("unbound")
        });
        println!("{}", serde_json::to_string(&value)?);
        return Ok(());
    }
    if !arguments.is_empty() {
        return Err("argv forbidden".into());
    }
    let mut request = Vec::new();
    std::io::stdin()
        .take(MAX_REQUEST_BYTES + 1)
        .read_to_end(&mut request)?;
    if request.is_empty() || request.len() as u64 > MAX_REQUEST_BYTES {
        return Err("invalid bounded request".into());
    }
    let request: ProbeRequestV1 = serde_json::from_slice(&request)?;
    if request.format != 1
        || request.kind != "probe-candidate-descriptor"
        || !valid_host_id(&request.source_host_id)
        || !valid_host_id(&request.relay_host_id)
    {
        return Err("invalid probe authority".into());
    }
    let descriptor = decode_hex(&request.candidate_descriptor_hex, 32_768)?;
    let verifier_context = decode_hex32(&request.verifier_context)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let receipt = runtime.block_on(probe(request, descriptor, verifier_context))?;
    println!("{}", serde_json::to_string(&receipt)?);
    Ok(())
}

async fn probe(
    request: ProbeRequestV1,
    descriptor: Vec<u8>,
    verifier_context: [u8; 32],
) -> Result<ProbeReceiptV2, Box<dyn std::error::Error>> {
    let resolver = Arc::new(SystemPublicEndpointResolver::new(2)?);
    let preparer = ReachabilityAdmissionPreparer::new(resolver.clone(), 2)?;
    let validator = ReachabilityDialValidator::new(resolver, 2)?;
    let replay = Arc::new(InMemoryReachabilityReplayStore::default());
    let mut admission = ReachabilityAdmission::new(replay);
    let now = unix_now()?;
    let deadline = Instant::now() + PROBE_DEADLINE;
    let prepared = preparer
        .prepare_descriptor(&descriptor, now, deadline)
        .await?;
    let pending = admission.register_prepared_descriptor(prepared, verifier_context, now)?;
    let signer = SigningKey::generate(&mut OsRng);
    let mut proofs = Vec::with_capacity(pending.challenges().len());
    let mut observations = Vec::with_capacity(pending.challenges().len());
    for endpoint_index in 0..pending.challenges().len() {
        let route = validator
            .validate_possession_dial(&pending, endpoint_index, deadline)
            .await?;
        let transport = route.challenge().transport;
        let proof = prove_relay_possession(&route, &signer, unix_now()?, deadline).await?;
        let proof_bytes = encode_relay_control(&RelayControlV1::PossessionProof(proof.clone()))?;
        let transcript = canonical_transcript(
            &descriptor,
            request.source_host_id.as_bytes(),
            request.relay_host_id.as_bytes(),
            endpoint_index,
            &proof_bytes,
        );
        observations.push(EndpointProbeV2 {
            endpoint_index,
            possession_proof_blake3: blake3::hash(&proof_bytes).to_hex().to_string(),
            success: true,
            transcript_blake3: blake3::hash(&transcript).to_hex().to_string(),
            transport: transport_name(transport).to_owned(),
        });
        proofs.push(proof);
    }
    let validated = admission.complete_descriptor_admission(pending, &proofs, unix_now()?)?;
    Ok(ProbeReceiptV2 {
        descriptor_blake3: blake3::hash(&descriptor).to_hex().to_string(),
        format: 2,
        probes: observations,
        relay_descriptor_hex: encode_hex(&descriptor),
        relay_host_id: request.relay_host_id,
        relay_node_id: encode_hex(validated.canonical().relay_node_id.as_bytes()),
        source_host_id: request.source_host_id,
    })
}

fn canonical_transcript(
    descriptor: &[u8],
    source_host: &[u8],
    relay_host: &[u8],
    endpoint_index: usize,
    proof: &[u8],
) -> Vec<u8> {
    let mut output = b"onebrain/p5/relay-preflight-transcript/v2\0".to_vec();
    for value in [
        descriptor,
        source_host,
        relay_host,
        &endpoint_index.to_be_bytes(),
        proof,
    ] {
        output.extend_from_slice(&(value.len() as u64).to_be_bytes());
        output.extend_from_slice(value);
    }
    output
}

fn transport_name(value: RelayTransportV1) -> &'static str {
    match value {
        RelayTransportV1::QuicUdp => "quic-udp",
        RelayTransportV1::TlsTcp443 => "tls-tcp-443",
    }
}

fn valid_host_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn unix_now() -> Result<u64, std::time::SystemTimeError> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

fn decode_hex(value: &str, maximum: usize) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if value.is_empty()
        || value.len() > maximum.saturating_mul(2)
        || value.len() % 2 != 0
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_hexdigit() || byte.is_ascii_uppercase())
    {
        return Err("invalid canonical hex".into());
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).map_err(Into::into))
        .collect()
}

fn decode_hex32(value: &str) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    decode_hex(value, 32)?
        .try_into()
        .map_err(|_| "invalid 32-byte hex".into())
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
