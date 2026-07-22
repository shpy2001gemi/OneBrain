"""
Cross-source deduplication for the OneBrain Concept Registry.

Loads raw JSONL from all four sources, builds a cross-reference index
from Wikidata's external-ID properties, merges duplicates, and writes
a unified ``concepts_deduped.jsonl``.
"""

import json
import logging
from collections import defaultdict
from pathlib import Path
from typing import Any, Optional

from tqdm import tqdm

from config import (
    CATEGORY_PLACE,
    CATEGORY_SUBSTANCE,
    CATEGORY_TAXON,
    SOURCE_CHEBI,
    SOURCE_ENGLISH_DICT,
    SOURCE_GEONAMES,
    SOURCE_NCBI,
    SOURCE_WIKIDATA,
    WD_PROP_CHEBI,
    WD_PROP_GEONAMES,
    WD_PROP_NCBI,
    p31_to_category,
)

logger = logging.getLogger(__name__)

# Source priority: lower number = higher priority
# wd(0) > gn(1) > ncbi(2) > chebi(3)


def _load_wikidata(raw_dir: Path) -> list[dict[str, Any]]:
    """Load Wikidata JSONL records.

    Prefers wikidata_ranked.jsonl (quality-ranked top 10M) if available,
    otherwise falls back to wikidata.jsonl.

    Args:
        raw_dir: Directory containing wikidata JSONL files.

    Returns:
        List of normalised records.
    """
    ranked_path = raw_dir / "wikidata_ranked.jsonl"
    plain_path = raw_dir / "wikidata.jsonl"

    if ranked_path.exists():
        path = ranked_path
        logger.info("Using RANKED wikidata: %s", path)
    elif plain_path.exists():
        path = plain_path
        logger.info("Using plain wikidata: %s", path)
    else:
        logger.warning("No wikidata file found in %s", raw_dir)
        return []

    records: list[dict[str, Any]] = []
    with open(path, "r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                obj = json.loads(line)
            except (json.JSONDecodeError, ValueError):
                continue
            records.append({
                "ext_id": obj["qid"],
                "source": SOURCE_WIKIDATA,
                "canonical_form": f"Q{obj['qid']}",
                "name": obj.get("labels", {}).get("en", obj.get("description", "")),
                "labels": obj.get("labels", {}),
                "category": p31_to_category(obj.get("category", "")),
                "cross_refs": obj.get("cross_refs", {}),
            })
    return records


def _load_geonames(path: Path) -> list[dict[str, Any]]:
    """Load GeoNames JSONL records.

    Args:
        path: Path to geonames.jsonl.

    Returns:
        List of normalised records.
    """
    records: list[dict[str, Any]] = []
    if not path.exists():
        logger.warning("GeoNames file not found: %s", path)
        return records

    with open(path, "r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                obj = json.loads(line)
            except (json.JSONDecodeError, ValueError):
                continue
            records.append({
                "ext_id": obj["geonames_id"],
                "source": SOURCE_GEONAMES,
                "canonical_form": f"GN{obj['geonames_id']}",
                "name": obj["name"],
                "labels": obj.get("labels", {}),
                "category": CATEGORY_PLACE,
                "cross_refs": {},
            })
    return records


def _load_ncbi(path: Path) -> list[dict[str, Any]]:
    """Load NCBI taxonomy JSONL records.

    Args:
        path: Path to ncbi_taxonomy.jsonl.

    Returns:
        List of normalised records.
    """
    records: list[dict[str, Any]] = []
    if not path.exists():
        logger.warning("NCBI taxonomy file not found: %s", path)
        return records

    with open(path, "r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                obj = json.loads(line)
            except (json.JSONDecodeError, ValueError):
                continue
            records.append({
                "ext_id": obj["taxid"],
                "source": SOURCE_NCBI,
                "canonical_form": f"NCBI{obj['taxid']}",
                "name": obj["name"],
                "labels": obj.get("labels", {}),
                "category": CATEGORY_TAXON,
                "cross_refs": {},
            })
    return records


def _load_chebi(path: Path) -> list[dict[str, Any]]:
    """Load ChEBI JSONL records.

    Args:
        path: Path to chebi.jsonl.

    Returns:
        List of normalised records.
    """
    records: list[dict[str, Any]] = []
    if not path.exists():
        logger.warning("ChEBI file not found: %s", path)
        return records

    with open(path, "r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                obj = json.loads(line)
            except (json.JSONDecodeError, ValueError):
                continue
            records.append({
                "ext_id": obj["chebi_id"],
                "source": SOURCE_CHEBI,
                "canonical_form": f"CHEBI{obj['chebi_id']}",
                "name": obj["name"],
                "labels": obj.get("labels", {}),
                "category": CATEGORY_SUBSTANCE,
                "cross_refs": {},
            })
    return records


def _merge_labels(
    winner: dict[str, Any],
    loser: dict[str, Any],
) -> None:
    """Merge labels from the losing record into the winner.

    Labels already present on the winner are not overwritten.

    Args:
        winner: The record that survives deduplication.
        loser: The record being merged away.
    """
    for lang, label in loser.get("labels", {}).items():
        if lang not in winner["labels"]:
            winner["labels"][lang] = label


def deduplicate(raw_dir: Path, output_path: Path) -> dict[str, Any]:
    """Perform cross-source deduplication of raw JSONL files.

    Uses Wikidata cross-reference properties (P683 → ChEBI, P846 → NCBI,
    P1566 → GeoNames) to identify entities that appear in multiple sources.
    The highest-priority source (Wikidata > GeoNames > NCBI > ChEBI) wins,
    and labels from losing entries are merged into the winner.

    Args:
        raw_dir: Directory containing source JSONL files.
        output_path: Path for the deduplicated output JSONL.

    Returns:
        Statistics dict with keys: ``total_input``, ``duplicates_removed``,
        ``final_count``, and per-source counts.
    """
    output_path.parent.mkdir(parents=True, exist_ok=True)

    # Load all sources
    logger.info("Loading raw records ...")
    wd_records = _load_wikidata(raw_dir)
    gn_records = _load_geonames(raw_dir / "geonames.jsonl")
    ncbi_records = _load_ncbi(raw_dir / "ncbi_taxonomy.jsonl")
    chebi_records = _load_chebi(raw_dir / "chebi.jsonl")

    # Load English dictionary for merge
    en_dict_path = raw_dir / "english_dict.jsonl"
    en_dict_records: list[dict[str, Any]] = []
    if en_dict_path.exists():
        logger.info("Loading English dictionary for merge ...")
        with open(en_dict_path, "r", encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if not line:
                    continue
                try:
                    obj = json.loads(line)
                except (json.JSONDecodeError, ValueError):
                    continue
                en_dict_records.append(obj)
        logger.info("Loaded %d English dictionary entries", len(en_dict_records))

    total_input = (len(wd_records) + len(gn_records) + len(ncbi_records)
                   + len(chebi_records) + len(en_dict_records))
    logger.info(
        "Loaded %d records total (wd=%d, gn=%d, ncbi=%d, chebi=%d, en=%d)",
        total_input,
        len(wd_records),
        len(gn_records),
        len(ncbi_records),
        len(chebi_records),
        len(en_dict_records),
    )

    # -----------------------------------------------------------------------
    # Build cross-reference index from Wikidata records
    # Maps (source, ext_id) → wikidata record index
    # -----------------------------------------------------------------------
    logger.info("Building cross-reference index …")

    # Secondary source key → Wikidata record
    xref_index: dict[tuple[int, int], int] = {}

    for i, rec in enumerate(wd_records):
        cross_refs = rec.get("cross_refs", {})

        # P683 → ChEBI ID
        chebi_ref = cross_refs.get(WD_PROP_CHEBI)
        if chebi_ref:
            try:
                xref_index[(SOURCE_CHEBI, int(chebi_ref))] = i
            except ValueError:
                pass

        # P846 → NCBI taxonomy ID
        ncbi_ref = cross_refs.get(WD_PROP_NCBI)
        if ncbi_ref:
            try:
                xref_index[(SOURCE_NCBI, int(ncbi_ref))] = i
            except ValueError:
                pass

        # P1566 → GeoNames ID
        geo_ref = cross_refs.get(WD_PROP_GEONAMES)
        if geo_ref:
            try:
                xref_index[(SOURCE_GEONAMES, int(geo_ref))] = i
            except ValueError:
                pass

    logger.info("Cross-reference index has %d entries", len(xref_index))

    # -----------------------------------------------------------------------
    # Deduplicate: mark secondary records that match Wikidata cross-refs
    # -----------------------------------------------------------------------
    duplicates_removed = 0

    # Process GeoNames
    deduped_gn: list[dict[str, Any]] = []
    for rec in tqdm(gn_records, desc="Dedup GeoNames", unit=" rec"):
        key = (SOURCE_GEONAMES, rec["ext_id"])
        if key in xref_index:
            wd_idx = xref_index[key]
            _merge_labels(wd_records[wd_idx], rec)
            duplicates_removed += 1
        else:
            deduped_gn.append(rec)

    # Process NCBI
    deduped_ncbi: list[dict[str, Any]] = []
    for rec in tqdm(ncbi_records, desc="Dedup NCBI", unit=" rec"):
        key = (SOURCE_NCBI, rec["ext_id"])
        if key in xref_index:
            wd_idx = xref_index[key]
            _merge_labels(wd_records[wd_idx], rec)
            duplicates_removed += 1
        else:
            deduped_ncbi.append(rec)

    # Process ChEBI
    deduped_chebi: list[dict[str, Any]] = []
    for rec in tqdm(chebi_records, desc="Dedup ChEBI", unit=" rec"):
        key = (SOURCE_CHEBI, rec["ext_id"])
        if key in xref_index:
            wd_idx = xref_index[key]
            _merge_labels(wd_records[wd_idx], rec)
            duplicates_removed += 1
        else:
            deduped_chebi.append(rec)

    # -----------------------------------------------------------------------
    # Merge English Dictionary (Strategy B)
    # Matching labels -> enrich Wikidata record; non-matching -> new entries
    # -----------------------------------------------------------------------
    en_merged = 0
    en_new: list[dict[str, Any]] = []

    if en_dict_records:
        logger.info("Merging English dictionary (Strategy B) ...")
        # Build label -> wd_record index for fast lookup
        wd_label_index: dict[str, int] = {}
        for i, rec in enumerate(wd_records):
            en_label = rec.get("labels", {}).get("en", "").lower()
            if en_label:
                wd_label_index[en_label] = i

        for en_rec in tqdm(en_dict_records, desc="Merge EN dict", unit=" rec"):
            en_label = en_rec.get("labels", {}).get("en", "").lower()
            if en_label in wd_label_index:
                # Enrich existing Wikidata record
                wd_idx = wd_label_index[en_label]
                wd_rec = wd_records[wd_idx]
                # Add POS, synonyms, hypernyms from dictionary
                if "pos" not in wd_rec:
                    wd_rec["pos"] = en_rec.get("pos", [])
                if "synonyms" not in wd_rec:
                    wd_rec["synonyms"] = en_rec.get("synonyms", [])
                if "hypernyms" not in wd_rec:
                    wd_rec["hypernyms"] = en_rec.get("hypernyms", [])
                en_merged += 1
            else:
                # New entry from dictionary
                en_new.append({
                    "ext_id": en_rec["id"],
                    "source": SOURCE_ENGLISH_DICT,
                    "canonical_form": en_rec["id"],
                    "name": en_rec.get("labels", {}).get("en", ""),
                    "labels": en_rec.get("labels", {}),
                    "category": 0,  # ENTITY
                    "pos": en_rec.get("pos", []),
                    "synonyms": en_rec.get("synonyms", []),
                    "hypernyms": en_rec.get("hypernyms", []),
                })

        logger.info("EN dict: %d merged into WD, %d new entries",
                    en_merged, len(en_new))

    # -----------------------------------------------------------------------
    # Write unified output
    # -----------------------------------------------------------------------
    all_records = wd_records + deduped_gn + deduped_ncbi + deduped_chebi + en_new
    final_count = len(all_records)

    logger.info("Writing %d deduplicated records to %s", final_count, output_path)

    with open(output_path, "w", encoding="utf-8") as out_fh:
        for rec in tqdm(all_records, desc="Writing deduped JSONL", unit=" rec"):
            # Strip internal cross_refs field from output
            output_rec = {
                "ext_id": rec["ext_id"],
                "source": rec["source"],
                "canonical_form": rec["canonical_form"],
                "name": rec["name"],
                "labels": rec["labels"],
                "category": rec["category"],
            }
            out_fh.write(json.dumps(output_rec, ensure_ascii=False) + "\n")

    stats = {
        "total_input": total_input,
        "duplicates_removed": duplicates_removed,
        "final_count": final_count,
        "per_source": {
            "wikidata": len(wd_records),
            "geonames": len(deduped_gn),
            "ncbi": len(deduped_ncbi),
            "chebi": len(deduped_chebi),
            "english_dict_merged": en_merged,
            "english_dict_new": len(en_new),
        },
    }

    logger.info("Deduplication stats: %s", json.dumps(stats, indent=2))
    return stats


if __name__ == "__main__":
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )
    from config import MERGED_DIR, RAW_DIR

    MERGED_DIR.mkdir(parents=True, exist_ok=True)
    deduplicate(RAW_DIR, MERGED_DIR / "concepts_deduped.jsonl")
