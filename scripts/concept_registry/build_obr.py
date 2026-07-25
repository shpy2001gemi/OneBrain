"""
OBR binary file builder for the OneBrain Concept Registry.

Reads ``concepts_deduped.jsonl`` and produces a compact binary
``.obr`` file with blake3-based CCIDs, matching the Rust-side
``ConceptRegistry`` layout.
"""

import json
import heapq
import logging
import struct
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import blake3
from tqdm import tqdm

from config import (
    OBR_MAGIC,
    OBR_VERSION,
    SOURCE_CHEBI,
    SOURCE_ENGLISH_DICT,
    SOURCE_GEONAMES,
    SOURCE_NAMES,
    SOURCE_NCBI,
    SOURCE_WIKIDATA,
)

logger = logging.getLogger(__name__)

# CCID prefix per source
_SOURCE_PREFIX: dict[int, str] = {
    SOURCE_WIKIDATA: "wd:Q",
    SOURCE_GEONAMES: "gn:",
    SOURCE_NCBI: "ncbi:",
    SOURCE_CHEBI: "chebi:",
    SOURCE_ENGLISH_DICT: "en:",
}

# Header: magic(4) + version(u32) + entry_count(u64) + label_count(u64) + reserved(8)
HEADER_SIZE = 32
HEADER_FORMAT = "<4sIQQ8s"
INDEX_VERSION = 1
INDEX_HEADER_SIZE = 64
INDEX_RECORD_SIZE = 24
INDEX_RECORD_FORMAT = "<16sQ"
LABEL_INDEX_MAGIC = b"OBLI"
CCID_INDEX_MAGIC = b"OBCI"
MANIFEST_VERSION = 1
BUILDER_VERSION = "onebrain-concept-registry-builder/1"
DEDUP_POLICY_VERSION = "crossref-label-dedup-v1"

_SOURCE_PROVENANCE: dict[int, tuple[str, str, str, str]] = {
    SOURCE_WIKIDATA: (
        "wikidata",
        "wikidata_ranked.jsonl",
        "https://dumps.wikimedia.org/wikidatawiki/entities/20260706/wikidata-20260706-all.json.gz",
        "CC0-1.0",
    ),
    SOURCE_GEONAMES: (
        "geonames",
        "geonames.jsonl",
        "https://download.geonames.org/export/dump/allCountries.zip",
        "CC-BY-4.0",
    ),
    SOURCE_NCBI: (
        "ncbi",
        "ncbi_taxonomy.jsonl",
        "https://ftp.ncbi.nlm.nih.gov/pub/taxonomy/taxdump.tar.gz",
        "US-GOV-PUBLIC-DOMAIN",
    ),
    SOURCE_CHEBI: (
        "chebi",
        "chebi.jsonl",
        "https://ftp.ebi.ac.uk/pub/databases/chebi/generic_dumps/",
        "CC-BY-4.0",
    ),
    SOURCE_ENGLISH_DICT: (
        "wordnet",
        "english_dict.jsonl",
        "https://wordnet.princeton.edu/",
        "WORDNET-3.0",
    ),
}


def _compute_ccid(source: int, ext_id: int | str) -> bytes:
    """Compute a 16-byte CCID using blake3.

    The CCID is the first 16 bytes of ``blake3(prefix + str(ext_id))``.

    Args:
        source: Source code (``SOURCE_WIKIDATA``, etc.).
        ext_id: External numeric ID or string ID.

    Returns:
        16-byte CCID digest.
    """
    prefix = _SOURCE_PREFIX.get(source, f"src{source}:")
    input_str = f"{prefix}{ext_id}"
    return blake3.blake3(input_str.encode("utf-8")).digest(length=16)


def _snapshot_id(path: Path) -> str:
    """Return a deterministic identifier for the exact local source snapshot."""
    if not path.exists() and path.name == "wikidata_ranked.jsonl":
        path = path.with_name("wikidata.jsonl")
    if not path.exists():
        return f"absent:{path.name}"
    stat = path.stat()
    return f"{path.name}:size={stat.st_size}:mtime_ns={stat.st_mtime_ns}"


def _blake3_file(path: Path) -> str:
    hasher = blake3.blake3()
    with open(path, "rb") as fh:
        while chunk := fh.read(1024 * 1024):
            hasher.update(chunk)
    return hasher.hexdigest()


def _label_key(label: str) -> bytes:
    return blake3.blake3(label.lower().encode("utf-8")).digest(length=16)


def _write_index_header(
    fh: Any, magic: bytes, record_count: int, obr_blake3: str
) -> None:
    fh.write(magic)
    fh.write(struct.pack("<I", INDEX_VERSION))
    fh.write(struct.pack("<Q", record_count))
    fh.write(bytes.fromhex(obr_blake3))
    fh.write(b"\x00" * 16)


def _build_sorted_index(
    unsorted_path: Path,
    output_path: Path,
    magic: bytes,
    obr_blake3: str,
    chunk_records: int = 500_000,
) -> dict[str, Any]:
    """External-sort fixed 24-byte records with bounded working memory."""
    size = unsorted_path.stat().st_size
    if size % INDEX_RECORD_SIZE:
        raise ValueError(f"Invalid temporary index length: {unsorted_path}")
    record_count = size // INDEX_RECORD_SIZE
    chunk_paths: list[Path] = []

    with tempfile.TemporaryDirectory(
        prefix=f"{output_path.name}.sort-", dir=output_path.parent
    ) as temp_directory:
        temp_root = Path(temp_directory)
        with open(unsorted_path, "rb") as source:
            chunk_number = 0
            while data := source.read(chunk_records * INDEX_RECORD_SIZE):
                records = [
                    data[pos : pos + INDEX_RECORD_SIZE]
                    for pos in range(0, len(data), INDEX_RECORD_SIZE)
                ]
                records.sort()
                chunk_path = temp_root / f"chunk-{chunk_number:06d}.bin"
                with open(chunk_path, "wb") as chunk:
                    chunk.writelines(records)
                chunk_paths.append(chunk_path)
                chunk_number += 1

        with open(output_path, "wb") as output:
            _write_index_header(output, magic, record_count, obr_blake3)
            handles = [open(path, "rb") for path in chunk_paths]
            try:
                heap: list[tuple[bytes, int]] = []
                for index, handle in enumerate(handles):
                    record = handle.read(INDEX_RECORD_SIZE)
                    if record:
                        heapq.heappush(heap, (record, index))
                while heap:
                    record, index = heapq.heappop(heap)
                    output.write(record)
                    next_record = handles[index].read(INDEX_RECORD_SIZE)
                    if next_record:
                        heapq.heappush(heap, (next_record, index))
            finally:
                for handle in handles:
                    handle.close()

    unsorted_path.unlink()
    return {
        "schema_version": INDEX_VERSION,
        "record_size": INDEX_RECORD_SIZE,
        "record_count": record_count,
        "blake3": _blake3_file(output_path),
        "file_size": output_path.stat().st_size,
    }


def _write_manifest(
    input_path: Path,
    output_path: Path,
    stats: dict[str, Any],
    per_source_counts: dict[int, int],
    label_index: dict[str, Any],
    ccid_index: dict[str, Any],
    obr_blake3: str,
) -> tuple[Path, str]:
    raw_dir = input_path.parent.parent / "raw"
    sources: dict[str, dict[str, Any]] = {}
    for source_code, (name, filename, uri, license_name) in _SOURCE_PROVENANCE.items():
        sources[name] = {
            "snapshot_id": _snapshot_id(raw_dir / filename),
            "source_uri": uri,
            "license": license_name,
            "record_count": per_source_counts.get(source_code, 0),
        }

    manifest = {
        "manifest_version": MANIFEST_VERSION,
        "obr_schema_version": OBR_VERSION,
        "builder_version": BUILDER_VERSION,
        "dedup_policy_version": DEDUP_POLICY_VERSION,
        "built_at_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "obr_blake3": obr_blake3,
        "entry_count": stats["entries"],
        "label_count": stats["labels"],
        "sources": sources,
        "label_index": label_index,
        "ccid_index": ccid_index,
    }
    manifest_path = Path(f"{output_path}.manifest.json")
    with open(manifest_path, "w", encoding="utf-8") as fh:
        json.dump(manifest, fh, ensure_ascii=False, indent=2, sort_keys=True)
        fh.write("\n")
    obr_stat = output_path.stat()
    label_index_stat = Path(f"{output_path}.labels.idx").stat()
    ccid_index_stat = Path(f"{output_path}.ccids.idx").stat()
    verification_path = Path(f"{output_path}.verification.json")
    with open(verification_path, "w", encoding="utf-8") as fh:
        json.dump(
            {
                "obr_blake3": obr_blake3,
                "file_size": obr_stat.st_size,
                "modified_ns": obr_stat.st_mtime_ns,
                "label_index": {
                    "blake3": label_index["blake3"],
                    "file_size": label_index_stat.st_size,
                    "modified_ns": label_index_stat.st_mtime_ns,
                },
                "ccid_index": {
                    "blake3": ccid_index["blake3"],
                    "file_size": ccid_index_stat.st_size,
                    "modified_ns": ccid_index_stat.st_mtime_ns,
                },
            },
            fh,
            indent=2,
            sort_keys=True,
        )
        fh.write("\n")
    return manifest_path, obr_blake3


def build(input_path: Path, output_path: Path) -> dict[str, Any]:
    """Build a binary .obr file from deduplicated JSONL.

    Binary format (all integers little-endian):

    - **Header** (32 bytes): magic(4B ``OBR1``) + version(u32) +
      entry_count(u64) + label_count(u64) + reserved(8B zeros)
    - **Per entry**: ccid(16B) + ext_id(u32) + source(u8) + category(u8) +
      name_len(u16) + name_bytes + num_labels(u16) +
      [label_len(u16) + label_bytes]*

    Args:
        input_path: Path to ``concepts_deduped.jsonl``.
        output_path: Path for the output ``.obr`` file.

    Returns:
        Statistics dict with keys: ``file_size``, ``entries``,
        ``labels``, ``collisions``.
    """
    output_path.parent.mkdir(parents=True, exist_ok=True)
    label_unsorted = Path(f"{output_path}.labels.idx.unsorted")
    ccid_unsorted = Path(f"{output_path}.ccids.idx.unsorted")
    label_index_path = Path(f"{output_path}.labels.idx")
    ccid_index_path = Path(f"{output_path}.ccids.idx")

    # -----------------------------------------------------------------------
    # Pass 1: Load all records, compute CCIDs, detect collisions
    # -----------------------------------------------------------------------
    logger.info("Loading records from %s …", input_path)
    records: list[dict[str, Any]] = []

    with open(input_path, "r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            records.append(json.loads(line))

    logger.info("Loaded %d records", len(records))

    # Collision detection
    ccid_map: dict[bytes, str] = {}  # ccid → canonical_form
    collisions: list[tuple[str, str]] = []

    entry_count = len(records)
    total_labels = 0
    per_source_counts: dict[int, int] = {}

    # -----------------------------------------------------------------------
    # Pass 2: Write binary
    # -----------------------------------------------------------------------
    logger.info("Writing OBR binary to %s …", output_path)

    with (
        open(output_path, "wb") as fh,
        open(label_unsorted, "wb") as label_fh,
        open(ccid_unsorted, "wb") as ccid_fh,
    ):
        # Write placeholder header (will be overwritten at end)
        fh.write(b"\x00" * HEADER_SIZE)

        for rec in tqdm(records, desc="Building OBR", unit=" entries"):
            entry_offset = fh.tell()
            source = rec["source"]
            per_source_counts[source] = per_source_counts.get(source, 0) + 1
            ext_id = rec["ext_id"]
            category = rec["category"]
            name = rec.get("name", "")
            labels = rec.get("labels", {})

            # Compute CCID
            ccid = _compute_ccid(source, ext_id)
            ccid_fh.write(struct.pack(INDEX_RECORD_FORMAT, ccid, entry_offset))

            # Collision check
            canonical = rec.get("canonical_form", f"{source}:{ext_id}")
            if ccid in ccid_map:
                collisions.append((ccid_map[ccid], canonical))
                logger.warning(
                    "CCID collision: %s vs %s (ccid=%s)",
                    ccid_map[ccid],
                    canonical,
                    ccid.hex(),
                )
            else:
                ccid_map[ccid] = canonical

            # Encode name
            name_bytes = name.encode("utf-8")
            name_len = len(name_bytes)

            # Collect label values (just the text, without language keys)
            label_values: list[bytes] = []
            for lang_val in labels.values():
                encoded = str(lang_val).encode("utf-8")
                label_values.append(encoded)
            num_labels = len(label_values)
            total_labels += num_labels

            indexed_labels = {name}
            indexed_labels.update(str(value) for value in labels.values())
            for indexed_label in indexed_labels:
                if indexed_label:
                    label_fh.write(
                        struct.pack(
                            INDEX_RECORD_FORMAT,
                            _label_key(indexed_label),
                            entry_offset,
                        )
                    )

            # Write entry
            # ccid(16B) + ext_id(u32) + source(u8) + category(u8) + name_len(u16)
            fh.write(ccid)
            # ext_id: convert string IDs to hash u32
            if isinstance(ext_id, int):
                fh.write(struct.pack("<I", ext_id))
            else:
                # Hash string ID to u32 for binary compatibility
                ext_id_hash = int.from_bytes(
                    blake3.blake3(str(ext_id).encode("utf-8")).digest(length=4),
                    "little",
                )
                fh.write(struct.pack("<I", ext_id_hash))
            fh.write(struct.pack("<B", source))
            fh.write(struct.pack("<B", category))
            fh.write(struct.pack("<H", name_len))
            fh.write(name_bytes)

            # num_labels(u16) + [label_len(u16) + label_bytes]*
            fh.write(struct.pack("<H", num_labels))
            for label_bytes_val in label_values:
                fh.write(struct.pack("<H", len(label_bytes_val)))
                fh.write(label_bytes_val)

        # ---------------------------------------------------------------
        # Rewrite header with actual counts
        # ---------------------------------------------------------------
        fh.seek(0)
        header = struct.pack(
            HEADER_FORMAT,
            OBR_MAGIC,
            OBR_VERSION,
            entry_count,
            total_labels,
            b"\x00" * 8,
        )
        fh.write(header)

    file_size = output_path.stat().st_size

    stats = {
        "file_size": file_size,
        "file_size_mb": round(file_size / (1024 * 1024), 2),
        "entries": entry_count,
        "labels": total_labels,
        "collisions": len(collisions),
    }
    obr_blake3 = _blake3_file(output_path)
    logger.info("Building bounded on-demand sidecar indexes …")
    label_index = _build_sorted_index(
        label_unsorted, label_index_path, LABEL_INDEX_MAGIC, obr_blake3
    )
    ccid_index = _build_sorted_index(
        ccid_unsorted, ccid_index_path, CCID_INDEX_MAGIC, obr_blake3
    )
    stats["label_index"] = label_index
    stats["ccid_index"] = ccid_index
    manifest_path, manifest_obr_blake3 = _write_manifest(
        input_path,
        output_path,
        stats,
        per_source_counts,
        label_index,
        ccid_index,
        obr_blake3,
    )
    stats["manifest_path"] = str(manifest_path)
    stats["obr_blake3"] = manifest_obr_blake3

    if collisions:
        logger.warning("Detected %d CCID collisions!", len(collisions))
        for c1, c2 in collisions[:10]:
            logger.warning("  Collision: %s ↔ %s", c1, c2)
    else:
        logger.info("No CCID collisions detected.")

    logger.info(
        "OBR build complete: %d entries, %d labels, %.2f MB, %d collisions",
        stats["entries"],
        stats["labels"],
        stats["file_size_mb"],
        stats["collisions"],
    )
    logger.info("Manifest: %s", manifest_path)
    return stats


if __name__ == "__main__":
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )
    from config import MERGED_DIR, OBR_OUTPUT

    OBR_OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    build(MERGED_DIR / "concepts_deduped.jsonl", OBR_OUTPUT)
