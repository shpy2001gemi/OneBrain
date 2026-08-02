use std::{env, fs, path::PathBuf};

use ciborium::value::Value;
use ed25519_dalek::{Signer, SigningKey};

const MANIFEST_BODY_DOMAIN: &[u8] = b"onebrain:concept-registry-manifest-body:1\0";
const REGISTRY_CHUNK_DOMAIN: &[u8] = b"onebrain:concept-registry-chunk:1\0";
const RELEASE_ID_DOMAIN: &[u8] = b"onebrain:concept-registry-release:1\0";
const RELEASE_ENVELOPE_DOMAIN: &[u8] = b"onebrain:concept-registry-envelope:1\0";
const CHANNEL_HEAD_BODY_DOMAIN: &[u8] = b"onebrain:concept-registry-channel-head-body:1\0";
const CHANNEL_HEAD_ENVELOPE_DOMAIN: &[u8] = b"onebrain:concept-registry-channel-head:1\0";
const CHANNEL_KEY_ID: &str = "channel-v1";
const RELEASE_KEY_ID: &str = "release-v1";

fn main() {
    let output = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: generate_registry_debug_fixture <output-directory>");
    fs::create_dir_all(&output).expect("create fixture output directory");

    let channel_signing = SigningKey::from_bytes(&[7; 32]);
    let release_signing = SigningKey::from_bytes(&[11; 32]);
    let runtime_range = runtime_range();
    let artifacts = [1_024u64, 2_048, 3_072]
        .into_iter()
        .enumerate()
        .map(|(role, length)| {
            let role = u8::try_from(role).expect("fixture role");
            let payload = vec![0x41 + role; usize::try_from(length).expect("fixture length")];
            map(vec![
                uint(u64::from(role)),
                uint(length),
                Value::Bytes(blake3::hash(&payload).as_bytes().to_vec()),
                Value::Array(vec![uint(1), uint(0)]),
                uint(1),
                Value::Array(vec![map(vec![
                    uint(0),
                    uint(length),
                    Value::Bytes(chunk_leaf(role, 0, &payload).to_vec()),
                ])]),
            ])
        })
        .collect();
    let manifest_body = encode(&map(vec![
        uint(1),
        uint(1),
        Value::Array(vec![uint(1), uint(0)]),
        Value::Text("fixture-1".into()),
        Value::Array(artifacts),
        runtime_range.clone(),
        uint(2_000_000_000),
        map(vec![Value::Null, Value::Null]),
        map(vec![
            Value::Bytes([51; 32].to_vec()),
            Value::Bytes([52; 32].to_vec()),
            Value::Bytes([53; 32].to_vec()),
        ]),
        map(vec![Value::Array(Vec::new())]),
    ]));
    let manifest_digest = domain_digest(MANIFEST_BODY_DOMAIN, &manifest_body);
    let release_id = domain_digest(RELEASE_ID_DOMAIN, &manifest_digest);

    let head_body = encode(&map(vec![
        uint(1),
        Value::Text("stable".into()),
        uint(1),
        uint(1),
        Value::Bytes(release_id.to_vec()),
        Value::Bytes(manifest_digest.to_vec()),
        runtime_range,
        uint(1),
    ]));
    let head_digest = domain_digest(CHANNEL_HEAD_BODY_DOMAIN, &head_body);

    let release_signature = release_signing
        .sign(&signature_input(
            RELEASE_ENVELOPE_DOMAIN,
            &[&release_id, &manifest_digest],
            RELEASE_KEY_ID,
        ))
        .to_bytes();
    let release_envelope = encode(&map(vec![
        uint(1),
        Value::Bytes(manifest_body),
        Value::Bytes(release_id.to_vec()),
        Value::Bytes(manifest_digest.to_vec()),
        uint(1),
        Value::Text(RELEASE_KEY_ID.into()),
        Value::Bytes(release_signature.to_vec()),
    ]));

    let channel_signature = channel_signing
        .sign(&signature_input(
            CHANNEL_HEAD_ENVELOPE_DOMAIN,
            &[&head_digest],
            CHANNEL_KEY_ID,
        ))
        .to_bytes();
    let channel_envelope = encode(&map(vec![
        uint(1),
        Value::Bytes(head_body),
        Value::Bytes(head_digest.to_vec()),
        uint(1),
        Value::Text(CHANNEL_KEY_ID.into()),
        Value::Bytes(channel_signature.to_vec()),
    ]));

    let trust_profile = encode(&map(vec![
        uint(1),
        uint(1),
        Value::Array(vec![map(vec![
            Value::Text(CHANNEL_KEY_ID.into()),
            Value::Bytes(channel_signing.verifying_key().to_bytes().to_vec()),
        ])]),
        Value::Array(vec![map(vec![
            Value::Text(RELEASE_KEY_ID.into()),
            Value::Bytes(release_signing.verifying_key().to_bytes().to_vec()),
        ])]),
        Value::Array(vec![map(vec![
            Value::Text("stable".into()),
            uint(1),
            Value::Bytes(head_digest.to_vec()),
            uint(1),
            Value::Bytes(release_id.to_vec()),
            Value::Bytes(manifest_digest.to_vec()),
        ])]),
        uint(1),
    ]));

    write_hex(
        output.join("registry_trust_profile.cbor.hex"),
        &trust_profile,
    );
    write_hex(
        output.join("registry_channel_head.cbor.hex"),
        &channel_envelope,
    );
    write_hex(output.join("registry_release.cbor.hex"), &release_envelope);
}

fn runtime_range() -> Value {
    map(vec![
        uint(7),
        uint(7),
        uint(1),
        uint(u64::from(u32::MAX)),
        uint(1),
        uint(u64::from(u32::MAX)),
    ])
}

fn map(values: Vec<Value>) -> Value {
    Value::Map(
        values
            .into_iter()
            .enumerate()
            .map(|(index, value)| (uint(u64::try_from(index).expect("map index")), value))
            .collect(),
    )
}

fn uint(value: u64) -> Value {
    Value::Integer(value.into())
}

fn encode(value: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    ciborium::ser::into_writer(value, &mut output).expect("encode deterministic fixture CBOR");
    output
}

fn domain_digest(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

fn chunk_leaf(role: u8, index: u32, bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(REGISTRY_CHUNK_DOMAIN);
    hasher.update(&[role]);
    hasher.update(&index.to_le_bytes());
    hasher.update(
        &u32::try_from(bytes.len())
            .expect("fixture chunk length")
            .to_le_bytes(),
    );
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

fn signature_input(domain: &[u8], digests: &[&[u8; 32]], key_id: &str) -> Vec<u8> {
    let mut message = domain.to_vec();
    for digest in digests {
        message.extend_from_slice(*digest);
    }
    message.extend_from_slice(&1u64.to_le_bytes());
    message.extend_from_slice(
        &u16::try_from(key_id.len())
            .expect("fixture key ID length")
            .to_le_bytes(),
    );
    message.extend_from_slice(key_id.as_bytes());
    message
}

fn write_hex(path: PathBuf, bytes: &[u8]) {
    fs::write(path, format!("{}\n", hex::encode(bytes))).expect("write fixture asset");
}
