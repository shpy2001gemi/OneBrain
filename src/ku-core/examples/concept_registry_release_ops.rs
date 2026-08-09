//! Narrow first-party process bridge for independently inspected release-cycle steps.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use ed25519_dalek::SigningKey;
use ku_core::{
    activate_concept_registry_release, package_concept_registry_release,
    parse_concept_registry_verifying_key, rollback_concept_registry_release,
    verify_concept_registry_release, ConceptRegistryReleasePackageInput,
    ConceptRegistryReleaseSource,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("concept-registry-release-ops: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    match required(&mut args, "OPERATION")?.as_str() {
        "package" => {
            let root = path(&mut args, "REGISTRY_ROOT")?;
            let obr = path(&mut args, "OBR")?;
            let sbom = path(&mut args, "SBOM")?;
            let sources: Vec<ConceptRegistryReleaseSource> =
                serde_json::from_slice(&fs::read(path(&mut args, "SOURCES")?)?)?;
            let release_id = required(&mut args, "RELEASE_ID")?;
            let key = read_key(&path(&mut args, "PRIVATE_KEY")?)?;
            package_concept_registry_release(
                &obr,
                &sbom,
                &root,
                ConceptRegistryReleasePackageInput {
                    release_id,
                    sources,
                },
                &key,
            )?;
        }
        "verify" => {
            let root = path(&mut args, "REGISTRY_ROOT")?;
            let release_id = required(&mut args, "RELEASE_ID")?;
            let key = parse_concept_registry_verifying_key(&required(&mut args, "PUBLIC_KEY")?)?;
            verify_concept_registry_release(&root.join("releases").join(release_id), &key)?;
        }
        "activate" => {
            let root = path(&mut args, "REGISTRY_ROOT")?;
            let release_id = required(&mut args, "RELEASE_ID")?;
            let key = parse_concept_registry_verifying_key(&required(&mut args, "PUBLIC_KEY")?)?;
            activate_concept_registry_release(&root, &release_id, &key)?;
        }
        "rollback" => {
            let root = path(&mut args, "REGISTRY_ROOT")?;
            let key = parse_concept_registry_verifying_key(&required(&mut args, "PUBLIC_KEY")?)?;
            rollback_concept_registry_release(&root, &key)?;
        }
        _ => return Err("unsupported release operation".into()),
    }
    if args.next().is_some() {
        return Err("unexpected release operation argument".into());
    }
    Ok(())
}

fn required(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String, Box<dyn Error>> {
    args.next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("missing {name}").into())
}

fn path(args: &mut impl Iterator<Item = String>, name: &str) -> Result<PathBuf, Box<dyn Error>> {
    Ok(PathBuf::from(required(args, name)?))
}

fn read_key(path: &Path) -> Result<SigningKey, Box<dyn Error>> {
    let value = fs::read_to_string(path)?;
    let value = value.trim();
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("private key must be 64 lowercase hexadecimal digits".into());
    }
    let mut bytes = [0u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    Ok(SigningKey::from_bytes(&bytes))
}
