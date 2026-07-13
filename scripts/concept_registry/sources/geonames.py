"""
GeoNames dump parser for the OneBrain Concept Registry.

Downloads the ``allCountries.zip`` dump (~400 MB) and parses the TSV
to extract geographic entities, writing results as JSONL.
"""

import io
import json
import logging
import zipfile
from pathlib import Path
from typing import Optional

import requests
from tqdm import tqdm

from config import CATEGORY_PLACE, GEONAMES_DUMP_URL

logger = logging.getLogger(__name__)

# GeoNames TSV column indices
COL_GEONAMEID = 0
COL_NAME = 1
COL_ASCIINAME = 2
COL_ALTERNATENAMES = 3
COL_LAT = 4
COL_LON = 5
COL_FEATURE_CLASS = 6
COL_FEATURE_CODE = 7
COL_COUNTRY_CODE = 8
COL_POPULATION = 14

# Feature classes to keep even with zero population
KEEP_FEATURE_CLASSES = {"A", "P", "T", "H", "L"}

# Max alternate names to store per entry
MAX_ALTERNATE_NAMES = 20


def _download_dump(dest_path: Path) -> None:
    """Download the GeoNames allCountries.zip dump with progress bar.

    Args:
        dest_path: Destination path for the downloaded zip file.
    """
    logger.info("Downloading GeoNames dump from %s", GEONAMES_DUMP_URL)
    resp = requests.get(GEONAMES_DUMP_URL, stream=True, timeout=600)
    resp.raise_for_status()

    total_size = int(resp.headers.get("content-length", 0))
    dest_path.parent.mkdir(parents=True, exist_ok=True)

    with (
        open(dest_path, "wb") as fh,
        tqdm(
            total=total_size,
            unit="B",
            unit_scale=True,
            desc="GeoNames download",
        ) as pbar,
    ):
        for chunk in resp.iter_content(chunk_size=1024 * 1024):
            fh.write(chunk)
            pbar.update(len(chunk))

    logger.info("GeoNames dump saved to %s", dest_path)


def _parse_population(value: str) -> int:
    """Safely parse a population string to int.

    Args:
        value: Raw string from the TSV population column.

    Returns:
        Parsed integer, or ``0`` on failure.
    """
    try:
        return int(value)
    except (ValueError, IndexError):
        return 0


def fetch_all(output_path: Path, checkpoint_dir: Path) -> int:
    """Parse the GeoNames allCountries dump and write JSONL output.

    Downloads the dump to *checkpoint_dir* if not already present,
    then streams through the TSV extracting populated places and
    notable geographic features.

    Args:
        output_path: Path to the output JSONL file.
        checkpoint_dir: Directory used for caching the downloaded zip.

    Returns:
        Total number of concepts written.
    """
    zip_path = checkpoint_dir / "allCountries.zip"

    if not zip_path.exists():
        _download_dump(zip_path)
    else:
        logger.info("GeoNames dump already cached at %s", zip_path)

    output_path.parent.mkdir(parents=True, exist_ok=True)
    total_written = 0

    logger.info("Parsing GeoNames dump …")
    with (
        zipfile.ZipFile(zip_path, "r") as zf,
        open(output_path, "w", encoding="utf-8") as out_fh,
    ):
        # The zip contains a single file: allCountries.txt
        inner_names = [n for n in zf.namelist() if n.endswith(".txt")]
        if not inner_names:
            raise FileNotFoundError("No .txt file found inside allCountries.zip")

        inner_name = inner_names[0]
        with zf.open(inner_name) as raw:
            reader = io.TextIOWrapper(raw, encoding="utf-8", errors="replace")
            for line in tqdm(reader, desc="GeoNames parse", unit=" lines"):
                line = line.rstrip("\n")
                if not line:
                    continue

                cols = line.split("\t")
                if len(cols) < 15:
                    continue

                feature_class = cols[COL_FEATURE_CLASS]
                population = _parse_population(cols[COL_POPULATION])
                alt_raw = cols[COL_ALTERNATENAMES]
                has_altnames = bool(alt_raw and alt_raw.strip())

                # Stricter filter to target ~1.5M (spec §9.3.2):
                # A (admin divisions): always keep (countries, provinces)
                # P (populated places): keep if population >= 500
                # T/H/L (terrain/water/parks): keep only if has alternate names (= notable)
                # Everything else (S, R, U, V): skip
                if feature_class == "A":
                    pass  # always keep
                elif feature_class == "P":
                    if population < 500:
                        continue
                elif feature_class in ("T", "H", "L"):
                    if not has_altnames:
                        continue
                else:
                    continue  # Skip S (spot), R (road), U (undersea), V (vegetation)

                geonames_id = int(cols[COL_GEONAMEID])
                name = cols[COL_NAME]
                country_code = cols[COL_COUNTRY_CODE]

                # Build labels from alternate names
                labels: dict[str, str] = {}
                alt_raw = cols[COL_ALTERNATENAMES]
                if alt_raw:
                    alts = alt_raw.split(",")
                    for i, alt in enumerate(alts):
                        alt = alt.strip()
                        if alt and i < MAX_ALTERNATE_NAMES:
                            labels[f"alt_{i}"] = alt

                # ASCII name as fallback English label
                ascii_name = cols[COL_ASCIINAME].strip()
                if ascii_name:
                    labels["en"] = ascii_name

                record = {
                    "geonames_id": geonames_id,
                    "name": name,
                    "labels": labels,
                    "category": "place",
                    "population": population,
                    "country_code": country_code,
                }
                out_fh.write(json.dumps(record, ensure_ascii=False) + "\n")
                total_written += 1

    logger.info("GeoNames parse complete: %d concepts written to %s", total_written, output_path)
    return total_written


if __name__ == "__main__":
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )
    from config import CHECKPOINT_DIR, RAW_DIR

    RAW_DIR.mkdir(parents=True, exist_ok=True)
    CHECKPOINT_DIR.mkdir(parents=True, exist_ok=True)
    fetch_all(RAW_DIR / "geonames.jsonl", CHECKPOINT_DIR)
