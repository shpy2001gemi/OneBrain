//! Validate, probe, and benchmark a compiled Concept Registry without loading it into RAM.

use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Instant;

use ku_core::concept_registry::{ConceptRegistry, ResolveResult};
use ku_core::concept_registry_manifest::{
    load_and_validate_manifest, load_and_validate_manifest_uncached,
};
use ku_core::indexed_concept_registry::IndexedConceptRegistry;
use serde::Serialize;

const PROBE_PROFILE: &str = "onebrain/concept-registry-probe/1";
const MAX_LABELS_FILE_BYTES: u64 = 1024 * 1024;
const MAX_PROBE_LABELS: usize = 10_000;
const MAX_LABEL_BYTES: usize = 4096;

#[derive(Clone, Copy)]
enum VerificationMode {
    Cached,
    Uncached,
}

impl VerificationMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Cached => "cached",
            Self::Uncached => "uncached",
        }
    }
}

#[derive(Serialize)]
struct ProbeReport {
    profile: &'static str,
    artifact_path: String,
    verification_mode: &'static str,
    cache_capacity: usize,
    labels_source: &'static str,
    sampled_from_obr: bool,
    ready_ms: u128,
    entry_count: u64,
    label_count: u64,
    obr_blake3: String,
    lookups: usize,
    found: usize,
    ambiguous: usize,
    missing: usize,
    first_lookup_us: u128,
    p50_us: u128,
    p95_us: u128,
    p99_us: u128,
    max_us: u128,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(args.next().ok_or(
        "usage: registry_probe <concepts.obr> [--sample N] [--labels-file PATH] \
         [--cache-capacity N] [--verification-cache cached|uncached] \
         [--summary-only] [--json] [labels...]",
    )?);
    let mut labels = Vec::new();
    let mut sample_size = None;
    let mut summary_only = false;
    let mut json_output = false;
    let mut cache_capacity = 4096usize;
    let mut verification_mode = VerificationMode::Cached;
    let mut labels_file_used = false;
    let mut positional_labels_used = false;
    while let Some(value) = args.next() {
        if value == "--sample" {
            let value = args
                .next()
                .ok_or("--sample requires a positive integer")?
                .to_string_lossy()
                .parse::<usize>()?;
            if value == 0 || value > MAX_PROBE_LABELS {
                return Err(format!("--sample must be between 1 and {MAX_PROBE_LABELS}").into());
            }
            sample_size = Some(value);
        } else if value == "--labels-file" {
            let labels_path = PathBuf::from(args.next().ok_or("--labels-file requires a path")?);
            for label in load_labels_file(&labels_path)? {
                push_checked_label(&mut labels, label)?;
            }
            labels_file_used = true;
        } else if value == "--cache-capacity" {
            cache_capacity = args
                .next()
                .ok_or("--cache-capacity requires a non-negative integer")?
                .to_string_lossy()
                .parse::<usize>()?;
        } else if value == "--verification-cache" {
            verification_mode = match args
                .next()
                .ok_or("--verification-cache requires cached or uncached")?
                .to_string_lossy()
                .as_ref()
            {
                "cached" => VerificationMode::Cached,
                "uncached" => VerificationMode::Uncached,
                _ => return Err("--verification-cache requires cached or uncached".into()),
            };
        } else if value == "--summary-only" {
            summary_only = true;
        } else if value == "--json" {
            json_output = true;
            summary_only = true;
        } else if value.to_string_lossy().starts_with("--") {
            return Err(format!("unknown option: {}", value.to_string_lossy()).into());
        } else {
            push_checked_label(&mut labels, value.to_string_lossy().into_owned())?;
            positional_labels_used = true;
        }
    }
    if let Some(sample_size) = sample_size {
        for label in sample_canonical_labels(&path, sample_size)? {
            push_checked_label(&mut labels, label)?;
        }
    }
    let labels = if labels.is_empty() {
        vec!["water".to_string(), "human".to_string(), "Mars".to_string()]
    } else {
        labels
    };
    let labels_source = match (
        sample_size.is_some(),
        labels_file_used,
        positional_labels_used,
    ) {
        (false, false, false) => "default",
        (false, true, false) => "external-file",
        (false, false, true) => "command-line",
        (true, false, false) => "obr-sample",
        _ => "mixed",
    };

    let started = Instant::now();
    let header = ConceptRegistry::inspect_obr(&path)?;
    let manifest = match verification_mode {
        VerificationMode::Cached => load_and_validate_manifest(&path, header)?,
        VerificationMode::Uncached => load_and_validate_manifest_uncached(&path, header)?,
    };
    let registry = IndexedConceptRegistry::open(&path, &manifest, cache_capacity)?;
    let ready_ms = started.elapsed().as_millis();
    if !json_output {
        println!(
            "ready_ms={} entries={} labels={} checksum={} verification={} cache_capacity={}",
            ready_ms,
            header.entry_count,
            header.label_count,
            manifest.obr_blake3,
            verification_mode.as_str(),
            cache_capacity,
        );
    }

    let mut lookup_micros = Vec::with_capacity(labels.len());
    let mut found = 0usize;
    let mut ambiguous = 0usize;
    let mut missing = 0usize;
    for label in labels {
        let lookup_started = Instant::now();
        let result = registry.resolve_checked(&label)?;
        let elapsed = lookup_started.elapsed().as_micros();
        lookup_micros.push(elapsed);
        match result {
            ResolveResult::Found(concept) => {
                found += 1;
                if !summary_only {
                    println!(
                        "label={label:?} status=FOUND ccid={} qid={} canonical={:?} lookup_us={elapsed}",
                        ku_core::ccid::ccid_to_hex(&concept.ccid),
                        concept.qid,
                        concept.canonical_name,
                    );
                }
            }
            ResolveResult::Ambiguous(concepts) => {
                ambiguous += 1;
                let preferred = &concepts[0];
                if !summary_only {
                    println!(
                        "label={label:?} status=AMBIGUOUS matches={} preferred_ccid={} preferred_qid={} preferred_canonical={:?} lookup_us={elapsed}",
                        concepts.len(),
                        ku_core::ccid::ccid_to_hex(&preferred.ccid),
                        preferred.qid,
                        preferred.canonical_name,
                    );
                }
            }
            ResolveResult::Fuzzy(concept) => {
                found += 1;
                if !summary_only {
                    println!(
                        "label={label:?} status=FUZZY ccid={} canonical={:?} lookup_us={elapsed}",
                        ku_core::ccid::ccid_to_hex(&concept.ccid),
                        concept.canonical_name,
                    );
                }
            }
            ResolveResult::NotFound => {
                missing += 1;
                if !summary_only {
                    println!("label={label:?} status=NOT_FOUND lookup_us={elapsed}");
                }
            }
        }
    }
    let first_lookup_us = lookup_micros.first().copied().unwrap_or(0);
    lookup_micros.sort_unstable();
    let report = ProbeReport {
        profile: PROBE_PROFILE,
        artifact_path: path.display().to_string(),
        verification_mode: verification_mode.as_str(),
        cache_capacity,
        labels_source,
        sampled_from_obr: sample_size.is_some(),
        ready_ms,
        entry_count: header.entry_count,
        label_count: header.label_count,
        obr_blake3: manifest.obr_blake3,
        lookups: lookup_micros.len(),
        found,
        ambiguous,
        missing,
        first_lookup_us,
        p50_us: percentile(&lookup_micros, 50),
        p95_us: percentile(&lookup_micros, 95),
        p99_us: percentile(&lookup_micros, 99),
        max_us: lookup_micros.last().copied().unwrap_or(0),
    };
    if json_output {
        println!("{}", serde_json::to_string(&report)?);
    } else {
        println!(
            "benchmark lookups={} found={} ambiguous={} missing={} first_us={} p50_us={} p95_us={} p99_us={} max_us={}",
            report.lookups,
            report.found,
            report.ambiguous,
            report.missing,
            report.first_lookup_us,
            report.p50_us,
            report.p95_us,
            report.p99_us,
            report.max_us,
        );
    }
    Ok(())
}

fn load_labels_file(path: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_LABELS_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_LABELS_FILE_BYTES {
        return Err(format!(
            "labels file exceeds {MAX_LABELS_FILE_BYTES} bytes: {}",
            path.display()
        )
        .into());
    }
    let mut labels = Vec::new();
    for line in String::from_utf8(bytes)?.lines() {
        let label = line.trim();
        if label.is_empty() {
            continue;
        }
        push_checked_label(&mut labels, label.to_owned())?;
    }
    if labels.is_empty() {
        return Err(format!("labels file has no probe labels: {}", path.display()).into());
    }
    Ok(labels)
}

fn push_checked_label(
    labels: &mut Vec<String>,
    label: String,
) -> Result<(), Box<dyn std::error::Error>> {
    if label.len() > MAX_LABEL_BYTES {
        return Err(format!("probe label exceeds {MAX_LABEL_BYTES} bytes").into());
    }
    if labels.len() >= MAX_PROBE_LABELS {
        return Err(format!("probe exceeds {MAX_PROBE_LABELS} labels").into());
    }
    labels.push(label);
    Ok(())
}

fn percentile(sorted: &[u128], percentile: usize) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((sorted.len() - 1) * percentile).div_ceil(100);
    sorted[index]
}

fn sample_canonical_labels(
    path: &Path,
    limit: usize,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(32))?;
    let mut labels = Vec::with_capacity(limit);
    let mut seen = HashSet::with_capacity(limit);
    while labels.len() < limit {
        let mut fixed = [0u8; 24];
        match file.read_exact(&mut fixed) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error.into()),
        }
        let name_len = u16::from_le_bytes(fixed[22..24].try_into()?) as usize;
        let mut name = vec![0u8; name_len];
        file.read_exact(&mut name)?;
        let name = String::from_utf8(name)?;
        if !name.is_empty() && seen.insert(name.clone()) {
            labels.push(name);
        }
        let mut count = [0u8; 2];
        file.read_exact(&mut count)?;
        for _ in 0..u16::from_le_bytes(count) {
            let mut length = [0u8; 2];
            file.read_exact(&mut length)?;
            file.seek(SeekFrom::Current(i64::from(u16::from_le_bytes(length))))?;
        }
    }
    Ok(labels)
}
