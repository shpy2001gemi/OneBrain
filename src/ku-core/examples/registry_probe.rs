//! Validate, probe, and benchmark a compiled Concept Registry without loading it into RAM.

use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Instant;

use ku_core::concept_registry::{ConceptRegistry, ResolveResult};
use ku_core::concept_registry_manifest::load_and_validate_manifest;
use ku_core::indexed_concept_registry::IndexedConceptRegistry;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path =
        PathBuf::from(args.next().ok_or(
            "usage: registry_probe <concepts.obr> [--sample N] [--summary-only] [labels...]",
        )?);
    let mut labels = Vec::new();
    let mut sample_size = None;
    let mut summary_only = false;
    while let Some(value) = args.next() {
        if value == "--sample" {
            sample_size = Some(
                args.next()
                    .ok_or("--sample requires a positive integer")?
                    .to_string_lossy()
                    .parse::<usize>()?,
            );
        } else if value == "--summary-only" {
            summary_only = true;
        } else {
            labels.push(value.to_string_lossy().into_owned());
        }
    }
    if let Some(sample_size) = sample_size {
        labels.extend(sample_canonical_labels(&path, sample_size)?);
    }
    let labels = if labels.is_empty() {
        vec!["water".to_string(), "human".to_string(), "Mars".to_string()]
    } else {
        labels
    };

    let started = Instant::now();
    let header = ConceptRegistry::inspect_obr(&path)?;
    let manifest = load_and_validate_manifest(&path, header)?;
    let registry = IndexedConceptRegistry::open(&path, &manifest, 4096)?;
    println!(
        "ready_ms={} entries={} labels={} checksum={}",
        started.elapsed().as_millis(),
        header.entry_count,
        header.label_count,
        manifest.obr_blake3
    );

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
    lookup_micros.sort_unstable();
    println!(
        "benchmark lookups={} found={} ambiguous={} missing={} p50_us={} p95_us={} p99_us={} max_us={}",
        lookup_micros.len(),
        found,
        ambiguous,
        missing,
        percentile(&lookup_micros, 50),
        percentile(&lookup_micros, 95),
        percentile(&lookup_micros, 99),
        lookup_micros.last().copied().unwrap_or(0),
    );
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
