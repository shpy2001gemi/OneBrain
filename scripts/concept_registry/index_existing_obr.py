"""Build bounded lookup sidecars and a manifest for an existing OBR1 file.

The OBR is opened read-only and never replaced. Sidecars are built under
``.building`` names and atomically renamed only after parsing and sorting
complete; the manifest is written last, so a node cannot accept a partial set.
"""

import argparse
import json
import logging
import struct
from pathlib import Path
from typing import Any

from tqdm import tqdm

from build_obr import (
    CCID_INDEX_MAGIC,
    INDEX_RECORD_FORMAT,
    LABEL_INDEX_MAGIC,
    _blake3_file,
    _build_sorted_index,
    _label_key,
    _write_manifest,
)
from config import MERGED_DIR, OBR_MAGIC, OBR_OUTPUT, OBR_VERSION

logger = logging.getLogger(__name__)
HEADER_FORMAT = "<4sIQQ8s"
HEADER_SIZE = struct.calcsize(HEADER_FORMAT)


def _read_exact(fh: Any, length: int, context: str) -> bytes:
    value = fh.read(length)
    if len(value) != length:
        raise ValueError(f"Truncated OBR while reading {context}")
    return value


def build_indexes(obr_path: Path, input_path: Path) -> dict[str, Any]:
    label_unsorted = Path(f"{obr_path}.labels.idx.unsorted")
    ccid_unsorted = Path(f"{obr_path}.ccids.idx.unsorted")
    label_building = Path(f"{obr_path}.labels.idx.building")
    ccid_building = Path(f"{obr_path}.ccids.idx.building")
    label_final = Path(f"{obr_path}.labels.idx")
    ccid_final = Path(f"{obr_path}.ccids.idx")

    with open(obr_path, "rb") as obr:
        magic, version, entry_count, label_count, _ = struct.unpack(
            HEADER_FORMAT, _read_exact(obr, HEADER_SIZE, "header")
        )
        if magic != OBR_MAGIC:
            raise ValueError(f"Invalid OBR magic: {magic!r}")
        if version != OBR_VERSION:
            raise ValueError(f"Unsupported OBR version: {version}")

        per_source_counts: dict[int, int] = {}
        with (
            open(label_unsorted, "wb") as label_output,
            open(ccid_unsorted, "wb") as ccid_output,
        ):
            for entry_index in tqdm(
                range(entry_count), desc="Indexing existing OBR", unit=" entries"
            ):
                offset = obr.tell()
                fixed = _read_exact(obr, 24, f"entry {entry_index} header")
                ccid = fixed[:16]
                source = fixed[20]
                name_len = struct.unpack("<H", fixed[22:24])[0]
                name = _read_exact(
                    obr, name_len, f"entry {entry_index} canonical name"
                ).decode("utf-8")
                num_labels = struct.unpack(
                    "<H", _read_exact(obr, 2, f"entry {entry_index} label count")
                )[0]
                labels = {name}
                for label_index in range(num_labels):
                    length = struct.unpack(
                        "<H",
                        _read_exact(
                            obr,
                            2,
                            f"entry {entry_index} label {label_index} length",
                        ),
                    )[0]
                    labels.add(
                        _read_exact(
                            obr, length, f"entry {entry_index} label {label_index}"
                        ).decode("utf-8")
                    )

                per_source_counts[source] = per_source_counts.get(source, 0) + 1
                ccid_output.write(struct.pack(INDEX_RECORD_FORMAT, ccid, offset))
                for label in labels:
                    if label:
                        label_output.write(
                            struct.pack(INDEX_RECORD_FORMAT, _label_key(label), offset)
                        )

        trailing = obr.read(1)
        if trailing:
            raise ValueError("OBR contains trailing bytes after declared entries")

    logger.info("Hashing OBR for artifact binding …")
    obr_blake3 = _blake3_file(obr_path)
    label_index = _build_sorted_index(
        label_unsorted, label_building, LABEL_INDEX_MAGIC, obr_blake3
    )
    ccid_index = _build_sorted_index(
        ccid_unsorted, ccid_building, CCID_INDEX_MAGIC, obr_blake3
    )

    label_building.replace(label_final)
    ccid_building.replace(ccid_final)
    stats: dict[str, Any] = {
        "file_size": obr_path.stat().st_size,
        "file_size_mb": round(obr_path.stat().st_size / (1024 * 1024), 2),
        "entries": entry_count,
        "labels": label_count,
        "collisions": 0,
        "label_index": label_index,
        "ccid_index": ccid_index,
    }
    manifest_path, _ = _write_manifest(
        input_path,
        obr_path,
        stats,
        per_source_counts,
        label_index,
        ccid_index,
        obr_blake3,
    )
    stats["manifest_path"] = str(manifest_path)
    stats["obr_blake3"] = obr_blake3
    return stats


def write_stamp_from_manifest(obr_path: Path) -> Path:
    manifest_path = Path(f"{obr_path}.manifest.json")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    stat = obr_path.stat()
    label_path = Path(f"{obr_path}.labels.idx")
    ccid_path = Path(f"{obr_path}.ccids.idx")
    label_stat = label_path.stat()
    ccid_stat = ccid_path.stat()
    stamp_path = Path(f"{obr_path}.verification.json")
    stamp_path.write_text(
        json.dumps(
            {
                "obr_blake3": manifest["obr_blake3"],
                "file_size": stat.st_size,
                "modified_ns": stat.st_mtime_ns,
                "label_index": {
                    "blake3": manifest["label_index"]["blake3"],
                    "file_size": label_stat.st_size,
                    "modified_ns": label_stat.st_mtime_ns,
                },
                "ccid_index": {
                    "blake3": manifest["ccid_index"]["blake3"],
                    "file_size": ccid_stat.st_size,
                    "modified_ns": ccid_stat.st_mtime_ns,
                },
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )
    return stamp_path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--obr", type=Path, default=OBR_OUTPUT)
    parser.add_argument(
        "--stamp-only",
        action="store_true",
        help="Write a verification stamp from an already validated manifest.",
    )
    parser.add_argument(
        "--input",
        type=Path,
        default=MERGED_DIR / "concepts_deduped.jsonl",
        help="Deduplicated input path used only to locate source snapshot metadata.",
    )
    args = parser.parse_args()
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )
    obr_path = args.obr.resolve()
    if args.stamp_only:
        logger.info("Verification stamp: %s", write_stamp_from_manifest(obr_path))
        return
    stats = build_indexes(obr_path, args.input.resolve())
    logger.info("Indexed %d entries", stats["entries"])
    logger.info("Label index: %s", stats["label_index"])
    logger.info("CCID index: %s", stats["ccid_index"])
    logger.info("Manifest: %s", stats["manifest_path"])


if __name__ == "__main__":
    main()
