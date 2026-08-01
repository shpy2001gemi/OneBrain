//! Offline operator CLI for signed Concept Registry releases.

use std::env;
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use ed25519_dalek::SigningKey;
use ku_core::{
    activate_concept_registry_release, concept_registry_release_capacity,
    package_concept_registry_release, parse_concept_registry_verifying_key,
    resolve_active_concept_registry_release, rollback_concept_registry_release,
    verify_concept_registry_release, ConceptRegistryReleasePackageInput,
    ConceptRegistryReleaseSource,
};
use rand::rngs::OsRng;

fn main() {
    if let Err(error) = run() {
        eprintln!("concept-registry-release: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_default();
    match command.as_str() {
        "keygen" => {
            let private_path = required_path(&mut args, "PRIVATE_KEY_FILE")?;
            let public_path = required_path(&mut args, "PUBLIC_KEY_FILE")?;
            no_more_args(args)?;
            let signing_key = SigningKey::generate(&mut OsRng);
            write_secret_key(&private_path, &encode_hex(&signing_key.to_bytes()))?;
            write_new_text(
                &public_path,
                &encode_hex(signing_key.verifying_key().as_bytes()),
            )?;
            println!("created signing key: {}", private_path.display());
            println!("created public key:  {}", public_path.display());
        }
        "package" => {
            let registry_root = required_path(&mut args, "REGISTRY_ROOT")?;
            let release_id = required_arg(&mut args, "RELEASE_ID")?;
            let obr_path = required_path(&mut args, "OBR_PATH")?;
            let sbom_path = required_path(&mut args, "SPDX_SBOM_PATH")?;
            let sources_path = required_path(&mut args, "SOURCES_JSON_PATH")?;
            let private_key_path = required_path(&mut args, "PRIVATE_KEY_FILE")?;
            no_more_args(args)?;
            let sources: Vec<ConceptRegistryReleaseSource> =
                serde_json::from_slice(&fs::read(sources_path)?)?;
            let signing_key = read_signing_key(&private_key_path)?;
            let stamp = package_concept_registry_release(
                &obr_path,
                &sbom_path,
                &registry_root,
                ConceptRegistryReleasePackageInput {
                    release_id,
                    sources,
                },
                &signing_key,
            )?;
            println!("{}", serde_json::to_string_pretty(&stamp)?);
        }
        "verify" => {
            let release_dir = required_path(&mut args, "RELEASE_DIR")?;
            let public_key_path = required_path(&mut args, "PUBLIC_KEY_FILE")?;
            no_more_args(args)?;
            let public_key = read_public_key(&public_key_path)?;
            let stamp = verify_concept_registry_release(&release_dir, &public_key)?;
            println!("{}", serde_json::to_string_pretty(&stamp)?);
        }
        "capacity" => {
            let registry_root = required_path(&mut args, "REGISTRY_ROOT")?;
            let obr_path = required_path(&mut args, "OBR_PATH")?;
            let sbom_path = required_path(&mut args, "SPDX_SBOM_PATH")?;
            no_more_args(args)?;
            let capacity =
                concept_registry_release_capacity(&obr_path, &sbom_path, &registry_root)?;
            println!("{}", serde_json::to_string_pretty(&capacity)?);
        }
        "activate" => {
            let registry_root = required_path(&mut args, "REGISTRY_ROOT")?;
            let release_id = required_arg(&mut args, "RELEASE_ID")?;
            let public_key_path = required_path(&mut args, "PUBLIC_KEY_FILE")?;
            no_more_args(args)?;
            let public_key = read_public_key(&public_key_path)?;
            let state =
                activate_concept_registry_release(&registry_root, &release_id, &public_key)?;
            println!("{}", serde_json::to_string_pretty(&state)?);
        }
        "rollback" => {
            let registry_root = required_path(&mut args, "REGISTRY_ROOT")?;
            let public_key_path = required_path(&mut args, "PUBLIC_KEY_FILE")?;
            no_more_args(args)?;
            let public_key = read_public_key(&public_key_path)?;
            let state = rollback_concept_registry_release(&registry_root, &public_key)?;
            println!("{}", serde_json::to_string_pretty(&state)?);
        }
        "status" => {
            let registry_root = required_path(&mut args, "REGISTRY_ROOT")?;
            let public_key_path = required_path(&mut args, "PUBLIC_KEY_FILE")?;
            no_more_args(args)?;
            let public_key = read_public_key(&public_key_path)?;
            let active = resolve_active_concept_registry_release(&registry_root, &public_key)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "generation": active.generation,
                    "active_release": active.release_id,
                    "previous_release": active.previous_release,
                    "release_dir": active.release_dir,
                    "obr_path": active.obr_path,
                    "artifact_root": active.stamp.artifact_root,
                    "source_root": active.stamp.source_root,
                    "signer_public_key": active.stamp.signer_public_key,
                }))?
            );
        }
        _ => return Err(usage().into()),
    }
    Ok(())
}

fn required_arg(
    args: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<String, Box<dyn Error>> {
    args.next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("missing {name}\n{}", usage()).into())
}

fn required_path(
    args: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<PathBuf, Box<dyn Error>> {
    Ok(PathBuf::from(required_arg(args, name)?))
}

fn no_more_args(mut args: impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
    if let Some(extra) = args.next() {
        return Err(format!("unexpected argument: {extra}\n{}", usage()).into());
    }
    Ok(())
}

fn read_signing_key(path: &Path) -> Result<SigningKey, Box<dyn Error>> {
    let value = fs::read_to_string(path)?;
    let bytes = decode_hex_32(value.trim(), "private signing key")?;
    Ok(SigningKey::from_bytes(&bytes))
}

fn read_public_key(path: &Path) -> Result<ed25519_dalek::VerifyingKey, Box<dyn Error>> {
    let value = fs::read_to_string(path)?;
    Ok(parse_concept_registry_verifying_key(value.trim())?)
}

fn decode_hex_32(value: &str, label: &str) -> Result<[u8; 32], Box<dyn Error>> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(format!("{label} must be exactly 64 lowercase hex digits").into());
    }
    let mut bytes = [0u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    Ok(bytes)
}

fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(DIGITS[(byte >> 4) as usize] as char);
        value.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    value
}

fn write_new_text(path: &Path, value: &str) -> Result<(), Box<dyn Error>> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(value.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn write_secret_key(path: &Path, value: &str) -> Result<(), Box<dyn Error>> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(value.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn usage() -> &'static str {
    "usage:\n  concept_registry_release keygen PRIVATE_KEY_FILE PUBLIC_KEY_FILE\n  concept_registry_release package REGISTRY_ROOT RELEASE_ID OBR_PATH SPDX_SBOM_PATH SOURCES_JSON_PATH PRIVATE_KEY_FILE\n  concept_registry_release verify RELEASE_DIR PUBLIC_KEY_FILE\n  concept_registry_release capacity REGISTRY_ROOT OBR_PATH SPDX_SBOM_PATH\n  concept_registry_release activate REGISTRY_ROOT RELEASE_ID PUBLIC_KEY_FILE\n  concept_registry_release rollback REGISTRY_ROOT PUBLIC_KEY_FILE\n  concept_registry_release status REGISTRY_ROOT PUBLIC_KEY_FILE"
}
