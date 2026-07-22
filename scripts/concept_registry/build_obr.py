"""
OBR binary file builder for the OneBrain Concept Registry.

Reads ``concepts_deduped.jsonl`` and produces a compact binary
``.obr`` file with blake3-based CCIDs, matching the Rust-side
``ConceptRegistry`` layout.
"""

import json
import logging
import struct
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

    # -----------------------------------------------------------------------
    # Pass 2: Write binary
    # -----------------------------------------------------------------------
    logger.info("Writing OBR binary to %s …", output_path)

    with open(output_path, "wb") as fh:
        # Write placeholder header (will be overwritten at end)
        fh.write(b"\x00" * HEADER_SIZE)

        for rec in tqdm(records, desc="Building OBR", unit=" entries"):
            source = rec["source"]
            ext_id = rec["ext_id"]
            category = rec["category"]
            name = rec.get("name", "")
            labels = rec.get("labels", {})

            # Compute CCID
            ccid = _compute_ccid(source, ext_id)

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
    return stats


if __name__ == "__main__":
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )
    from config import MERGED_DIR, OBR_OUTPUT

    OBR_OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    build(MERGED_DIR / "concepts_deduped.jsonl", OBR_OUTPUT)
