//! Closed relay-service configuration and offline lifecycle modes.

use std::collections::BTreeSet;
#[cfg(unix)]
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Write;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signer, SigningKey};
use onebrain_protocol::{
    encode_reachability_object, reachability_signing_bytes, HostAddressV1, ProtocolVersionV1,
    ReachabilityObjectV1, ReachabilitySignatureRoleV1, RelayDescriptorV1, RelayEndpointV1,
    RelayTransportV1,
};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::{principal_node_id, DurableRelayState, DurableStateKind};

const MAX_CONFIG_BYTES: u64 = 65_536;
const STATE_FILE: &str = "relay-state.redb";
const ACTIVATION_KEY: &[u8] = b"descriptor-activation-v1";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelayConfigV1 {
    pub format: u64,
    pub data_root: PathBuf,
    pub signer_locator: PathBuf,
    pub udp_bind: Option<SocketAddr>,
    pub tcp443_bind: Option<SocketAddr>,
    pub advertised_endpoints: Vec<RelayConfiguredEndpointV1>,
    pub capacity_policy_digest: [u8; 32],
    pub max_reservations: usize,
    pub max_reservations_per_target: usize,
    pub max_rendezvous_records: usize,
    pub descriptor_sequence: u64,
    pub descriptor_issued_at: u64,
    pub descriptor_expires_at: u64,
    pub log_destination: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelayConfiguredEndpointV1 {
    pub transport: String,
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelayActivationProbeSetV1 {
    pub format: u64,
    pub descriptor_blake3: String,
    pub probes: Vec<RelayActivationProbeV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelayActivationProbeV1 {
    pub source_host_id: String,
    pub endpoint_index: usize,
    pub transport: String,
    pub success: bool,
    pub transcript_blake3: String,
}

pub fn generate_identity(output: &Path) -> Result<[u8; 32], RuntimeError> {
    let mut secret = [0u8; 32];
    OsRng.fill_bytes(&mut secret);
    let key = SigningKey::from_bytes(&secret);
    write_create_new(output, &secret)?;
    Ok(*key.verifying_key().as_bytes())
}

pub fn initialize_state(config_path: &Path) -> Result<(), RuntimeError> {
    let config = read_config(config_path)?;
    validate_config_shape(&config)?;
    std::fs::create_dir_all(&config.data_root).map_err(|_| RuntimeError::Io)?;
    DurableRelayState::initialize(&config.data_root.join(STATE_FILE))
        .map_err(|_| RuntimeError::State)?;
    Ok(())
}

pub fn verify_config(config_path: &Path) -> Result<RelayConfigV1, RuntimeError> {
    let config = read_config(config_path)?;
    validate_config_shape(&config)?;
    if !config.signer_locator.is_file() || !config.data_root.join(STATE_FILE).is_file() {
        return Err(RuntimeError::MissingState);
    }
    read_signing_key(&config.signer_locator)?;
    DurableRelayState::open(&config.data_root.join(STATE_FILE)).map_err(|_| RuntimeError::State)?;
    Ok(config)
}

pub fn export_candidate_descriptor(
    config_path: &Path,
    output: &Path,
) -> Result<[u8; 32], RuntimeError> {
    let config = verify_config(config_path)?;
    let signing_key = read_signing_key(&config.signer_locator)?;
    let mut descriptor = descriptor_from_config(&config, &signing_key)?;
    descriptor.relay_signature = signing_key
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::RelayDescriptor(descriptor.clone()),
                ReachabilitySignatureRoleV1::RelayDescriptor,
            )
            .map_err(|_| RuntimeError::Descriptor)?,
        )
        .to_bytes();
    let bytes = encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(descriptor))
        .map_err(|_| RuntimeError::Descriptor)?;
    write_create_new(output, &bytes)?;
    let digest = *blake3::hash(&bytes).as_bytes();
    let state = DurableRelayState::open(&config.data_root.join(STATE_FILE))
        .map_err(|_| RuntimeError::State)?;
    state
        .create_new(DurableStateKind::DescriptorFloor, b"candidate-v1", &digest)
        .map_err(|_| RuntimeError::State)?;
    Ok(digest)
}

pub fn activate_descriptor(
    config_path: &Path,
    probe_set_path: &Path,
) -> Result<[u8; 32], RuntimeError> {
    let config = verify_config(config_path)?;
    let signing_key = read_signing_key(&config.signer_locator)?;
    let mut descriptor = descriptor_from_config(&config, &signing_key)?;
    descriptor.relay_signature = signing_key
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::RelayDescriptor(descriptor.clone()),
                ReachabilitySignatureRoleV1::RelayDescriptor,
            )
            .map_err(|_| RuntimeError::Descriptor)?,
        )
        .to_bytes();
    let bytes = encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(descriptor))
        .map_err(|_| RuntimeError::Descriptor)?;
    let descriptor_digest = *blake3::hash(&bytes).as_bytes();
    let probe_bytes = read_bounded(probe_set_path)?;
    let probes: RelayActivationProbeSetV1 =
        serde_json::from_slice(&probe_bytes).map_err(|_| RuntimeError::ProbeSet)?;
    validate_probe_set(&config, &probes, descriptor_digest)?;
    let digest = *blake3::hash(&probe_bytes).as_bytes();
    let state = DurableRelayState::open(&config.data_root.join(STATE_FILE))
        .map_err(|_| RuntimeError::State)?;
    state
        .create_new(DurableStateKind::ControlFloor, ACTIVATION_KEY, &digest)
        .map_err(|_| RuntimeError::State)?;
    Ok(digest)
}

pub fn serve(config_path: &Path, preflight_only: bool) -> Result<(), RuntimeError> {
    let config = verify_config(config_path)?;
    let state = DurableRelayState::open(&config.data_root.join(STATE_FILE))
        .map_err(|_| RuntimeError::State)?;
    if !preflight_only
        && state
            .get(DurableStateKind::ControlFloor, ACTIVATION_KEY)
            .map_err(|_| RuntimeError::State)?
            .is_none()
    {
        return Err(RuntimeError::NotActivated);
    }
    // Task 8 owns the actual UDP/TLS listeners. Task 7 deliberately proves
    // that an unactivated process cannot enter control/data service.
    Ok(())
}

fn descriptor_from_config(
    config: &RelayConfigV1,
    signing_key: &SigningKey,
) -> Result<RelayDescriptorV1, RuntimeError> {
    let endpoints = config
        .advertised_endpoints
        .iter()
        .map(configured_endpoint)
        .collect::<Result<Vec<_>, _>>()?;
    let mut transports = endpoints
        .iter()
        .map(|value| value.transport)
        .collect::<Vec<_>>();
    transports.sort();
    transports.dedup();
    Ok(RelayDescriptorV1 {
        format: 1,
        relay_node_id: principal_node_id(signing_key.verifying_key().as_bytes()),
        relay_public_key: *signing_key.verifying_key().as_bytes(),
        endpoints,
        supported_transports: transports,
        protocol_versions: vec![ProtocolVersionV1 { major: 1, minor: 0 }],
        capacity_policy_digest: config.capacity_policy_digest,
        previous_descriptor_blake3: None,
        sequence: config.descriptor_sequence,
        issued_at: config.descriptor_issued_at,
        expires_at: config.descriptor_expires_at,
        relay_signature: [0; 64],
    })
}

fn validate_config_shape(config: &RelayConfigV1) -> Result<(), RuntimeError> {
    if config.format != 1
        || config.data_root.as_os_str().is_empty()
        || config.signer_locator.as_os_str().is_empty()
        || config.log_destination.as_os_str().is_empty()
        || config.advertised_endpoints.is_empty()
        || config.advertised_endpoints.len() > 8
        || config.max_reservations == 0
        || config.max_reservations_per_target == 0
        || config.max_reservations_per_target > config.max_reservations
        || config.max_rendezvous_records == 0
        || config.max_rendezvous_records > 256
        || config.descriptor_sequence == 0
        || config.descriptor_issued_at >= config.descriptor_expires_at
        || config.descriptor_expires_at - config.descriptor_issued_at > 600
        || (config.udp_bind.is_none() && config.tcp443_bind.is_none())
    {
        return Err(RuntimeError::Config);
    }
    for endpoint in &config.advertised_endpoints {
        configured_endpoint(endpoint)?;
    }
    Ok(())
}

fn configured_endpoint(value: &RelayConfiguredEndpointV1) -> Result<RelayEndpointV1, RuntimeError> {
    if value.port == 0 {
        return Err(RuntimeError::Config);
    }
    let transport = match value.transport.as_str() {
        "quic-udp" => RelayTransportV1::QuicUdp,
        "tls-tcp-443" if value.port == 443 => RelayTransportV1::TlsTcp443,
        _ => return Err(RuntimeError::Config),
    };
    let host = match value.host.parse::<IpAddr>() {
        Ok(IpAddr::V4(address)) if ipv4_is_global(address.octets()) => {
            HostAddressV1::Ipv4(address.octets())
        }
        Ok(IpAddr::V6(address)) if ipv6_is_global(address.octets()) => {
            HostAddressV1::Ipv6(address.octets())
        }
        Ok(_) => return Err(RuntimeError::Config),
        Err(_) if valid_dns(&value.host) => HostAddressV1::Dns(value.host.clone()),
        Err(_) => return Err(RuntimeError::Config),
    };
    Ok(RelayEndpointV1 {
        transport,
        host,
        port: value.port,
    })
}

fn validate_probe_set(
    config: &RelayConfigV1,
    probes: &RelayActivationProbeSetV1,
    descriptor_digest: [u8; 32],
) -> Result<(), RuntimeError> {
    if probes.format != 1 || decode_hex32(&probes.descriptor_blake3)? != descriptor_digest {
        return Err(RuntimeError::ProbeSet);
    }
    let mut hosts = BTreeSet::new();
    let mut covered = BTreeSet::new();
    for probe in &probes.probes {
        if !probe.success
            || probe.endpoint_index >= config.advertised_endpoints.len()
            || probe.transport != config.advertised_endpoints[probe.endpoint_index].transport
            || decode_hex32(&probe.transcript_blake3)? == [0; 32]
        {
            return Err(RuntimeError::ProbeSet);
        }
        hosts.insert(probe.source_host_id.as_str());
        covered.insert(probe.endpoint_index);
    }
    if hosts.len() < 2 || covered.len() != config.advertised_endpoints.len() {
        return Err(RuntimeError::ProbeSet);
    }
    Ok(())
}

fn valid_dns(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && value.is_ascii()
        && !value.starts_with('.')
        && !value.ends_with('.')
}

fn ipv4_is_global([a, b, c, d]: [u8; 4]) -> bool {
    !(a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (b == 18 || b == 19))
        || a >= 224
        || [a, b, c, d] == [255, 255, 255, 255])
}

fn ipv6_is_global(octets: [u8; 16]) -> bool {
    let unspecified = octets.iter().all(|byte| *byte == 0);
    let loopback = octets[..15].iter().all(|byte| *byte == 0) && octets[15] == 1;
    let multicast = octets[0] == 0xff;
    let unique_local = octets[0] & 0xfe == 0xfc;
    let link_local = octets[0] == 0xfe && octets[1] & 0xc0 == 0x80;
    let global_unicast = octets[0] & 0xe0 == 0x20;
    global_unicast && !unspecified && !loopback && !multicast && !unique_local && !link_local
}

fn read_config(path: &Path) -> Result<RelayConfigV1, RuntimeError> {
    serde_json::from_slice(&read_bounded(path)?).map_err(|_| RuntimeError::Config)
}

fn read_signing_key(path: &Path) -> Result<SigningKey, RuntimeError> {
    let bytes = read_bounded(path)?;
    let secret: [u8; 32] = bytes.try_into().map_err(|_| RuntimeError::Identity)?;
    Ok(SigningKey::from_bytes(&secret))
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, RuntimeError> {
    let metadata = std::fs::metadata(path).map_err(|_| RuntimeError::Io)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_CONFIG_BYTES {
        return Err(RuntimeError::Limit);
    }
    std::fs::read(path).map_err(|_| RuntimeError::Io)
}

fn write_create_new(path: &Path, bytes: &[u8]) -> Result<(), RuntimeError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| RuntimeError::OutputExists)?;
    file.write_all(bytes).map_err(|_| RuntimeError::Io)?;
    file.sync_all().map_err(|_| RuntimeError::Io)?;
    sync_parent(path)?;
    Ok(())
}

fn sync_parent(path: &Path) -> Result<(), RuntimeError> {
    let parent = path.parent().ok_or(RuntimeError::Io)?;
    #[cfg(unix)]
    {
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| RuntimeError::Io)
    }
    #[cfg(not(unix))]
    {
        let _ = parent;
        Ok(())
    }
}

fn decode_hex32(value: &str) -> Result<[u8; 32], RuntimeError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(RuntimeError::ProbeSet);
    }
    let mut output = [0; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(pair).map_err(|_| RuntimeError::ProbeSet)?;
        output[index] = u8::from_str_radix(text, 16).map_err(|_| RuntimeError::ProbeSet)?;
    }
    Ok(output)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeError {
    Config,
    Identity,
    Descriptor,
    ProbeSet,
    State,
    MissingState,
    NotActivated,
    OutputExists,
    Limit,
    Io,
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "OBP_RELAY_RUNTIME: {self:?}")
    }
}

impl std::error::Error for RuntimeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_is_create_new_and_activation_requires_two_remote_hosts() {
        let directory = tempfile::tempdir().unwrap();
        let key_path = directory.path().join("relay.key");
        generate_identity(&key_path).unwrap();
        assert_eq!(
            generate_identity(&key_path).unwrap_err(),
            RuntimeError::OutputExists
        );
        let data_root = directory.path().join("data");
        let config_path = directory.path().join("relay.json");
        let config = RelayConfigV1 {
            format: 1,
            data_root: data_root.clone(),
            signer_locator: key_path,
            udp_bind: Some("0.0.0.0:41000".parse().unwrap()),
            tcp443_bind: None,
            advertised_endpoints: vec![RelayConfiguredEndpointV1 {
                transport: "quic-udp".into(),
                host: "1.1.1.1".into(),
                port: 41000,
            }],
            capacity_policy_digest: [7; 32],
            max_reservations: 8,
            max_reservations_per_target: 3,
            max_rendezvous_records: 64,
            descriptor_sequence: 1,
            descriptor_issued_at: 100,
            descriptor_expires_at: 700,
            log_destination: directory.path().join("relay.log"),
        };
        std::fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
        initialize_state(&config_path).unwrap();
        assert_eq!(
            initialize_state(&config_path).unwrap_err(),
            RuntimeError::State
        );
        serve(&config_path, true).unwrap();
        assert_eq!(
            serve(&config_path, false).unwrap_err(),
            RuntimeError::NotActivated
        );
        let descriptor_path = directory.path().join("descriptor.cbor");
        let descriptor_digest =
            export_candidate_descriptor(&config_path, &descriptor_path).unwrap();
        let one_host = RelayActivationProbeSetV1 {
            format: 1,
            descriptor_blake3: encode_hex(&descriptor_digest),
            probes: vec![RelayActivationProbeV1 {
                source_host_id: "runner-b".into(),
                endpoint_index: 0,
                transport: "quic-udp".into(),
                success: true,
                transcript_blake3: encode_hex(&[8; 32]),
            }],
        };
        let bad_probe_path = directory.path().join("bad-probes.json");
        std::fs::write(&bad_probe_path, serde_json::to_vec(&one_host).unwrap()).unwrap();
        assert_eq!(
            activate_descriptor(&config_path, &bad_probe_path).unwrap_err(),
            RuntimeError::ProbeSet
        );
        let mut valid = one_host;
        valid.probes.push(RelayActivationProbeV1 {
            source_host_id: "runner-c".into(),
            endpoint_index: 0,
            transport: "quic-udp".into(),
            success: true,
            transcript_blake3: encode_hex(&[9; 32]),
        });
        let valid_probe_path = directory.path().join("valid-probes.json");
        std::fs::write(&valid_probe_path, serde_json::to_vec(&valid).unwrap()).unwrap();
        activate_descriptor(&config_path, &valid_probe_path).unwrap();
        serve(&config_path, false).unwrap();
    }

    fn encode_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
