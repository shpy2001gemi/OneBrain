"""
NCBI Taxonomy dump parser for the OneBrain Concept Registry.

Downloads ``taxdump.tar.gz`` from NCBI FTP and parses ``names.dmp``
and ``nodes.dmp`` to extract species and genus-level taxa as JSONL.
"""

import gzip
import io
import json
import logging
import tarfile
from pathlib import Path
from typing import Optional

import requests
from tqdm import tqdm

from config import CATEGORY_TAXON, NCBI_TAXDUMP_URL

logger = logging.getLogger(__name__)

# Ranks to keep
KEEP_RANKS = {"species", "genus"}


def _download_dump(dest_path: Path) -> None:
    """Download the NCBI taxdump.tar.gz with progress bar.

    Args:
        dest_path: Destination path for the downloaded archive.
    """
    logger.info("Downloading NCBI taxdump from %s", NCBI_TAXDUMP_URL)
    resp = requests.get(NCBI_TAXDUMP_URL, stream=True, timeout=600)
    resp.raise_for_status()

    total_size = int(resp.headers.get("content-length", 0))
    dest_path.parent.mkdir(parents=True, exist_ok=True)

    with (
        open(dest_path, "wb") as fh,
        tqdm(
            total=total_size,
            unit="B",
            unit_scale=True,
            desc="NCBI taxdump download",
        ) as pbar,
    ):
        for chunk in resp.iter_content(chunk_size=1024 * 1024):
            fh.write(chunk)
            pbar.update(len(chunk))

    logger.info("NCBI taxdump saved to %s", dest_path)


def _parse_dmp_line(line: str) -> list[str]:
    """Parse a pipe-tab-delimited .dmp line into fields.

    The NCBI .dmp format uses ``\\t|\\t`` as delimiter with a trailing
    ``\\t|`` at line end.

    Args:
        line: Raw line from a .dmp file.

    Returns:
        List of stripped field values.
    """
    # Strip trailing \t|\n
    line = line.rstrip("\n").rstrip("|").rstrip("\t")
    return [field.strip() for field in line.split("\t|\t")]


def fetch_all(output_path: Path, checkpoint_dir: Path) -> int:
    """Parse the NCBI taxonomy dump and write JSONL output.

    Downloads the dump to *checkpoint_dir* if not already present,
    then parses ``nodes.dmp`` (for ranks) and ``names.dmp`` (for names)
    to produce taxon records.

    Args:
        output_path: Path to the output JSONL file.
        checkpoint_dir: Directory used for caching the downloaded archive.

    Returns:
        Total number of concepts written.
    """
    tar_path = checkpoint_dir / "taxdump.tar.gz"

    if not tar_path.exists():
        _download_dump(tar_path)
    else:
        logger.info("NCBI taxdump already cached at %s", tar_path)

    output_path.parent.mkdir(parents=True, exist_ok=True)

    # -----------------------------------------------------------------------
    # Step 1: Parse nodes.dmp to identify which taxids are species/genus
    # -----------------------------------------------------------------------
    logger.info("Parsing nodes.dmp for rank filtering …")
    taxid_ranks: dict[int, str] = {}

    with tarfile.open(tar_path, "r:gz") as tf:
        nodes_member = None
        for member in tf.getmembers():
            if member.name == "nodes.dmp":
                nodes_member = member
                break

        if nodes_member is None:
            raise FileNotFoundError("nodes.dmp not found in taxdump archive")

        nodes_fh = tf.extractfile(nodes_member)
        if nodes_fh is None:
            raise FileNotFoundError("Could not extract nodes.dmp")

        reader = io.TextIOWrapper(nodes_fh, encoding="utf-8", errors="replace")
        for line in tqdm(reader, desc="nodes.dmp", unit=" lines"):
            fields = _parse_dmp_line(line)
            if len(fields) < 3:
                continue
            try:
                taxid = int(fields[0])
            except ValueError:
                continue
            rank = fields[2].strip().lower()
            if rank in KEEP_RANKS:
                taxid_ranks[taxid] = rank

    logger.info("Found %d taxa with kept ranks", len(taxid_ranks))

    # -----------------------------------------------------------------------
    # Step 2: Parse names.dmp to collect names for kept taxids
    # -----------------------------------------------------------------------
    logger.info("Parsing names.dmp for name collection …")

    # Per taxid: {"scientific": str, "common": [str, ...]}
    taxid_names: dict[int, dict[str, object]] = {}

    with tarfile.open(tar_path, "r:gz") as tf:
        names_member = None
        for member in tf.getmembers():
            if member.name == "names.dmp":
                names_member = member
                break

        if names_member is None:
            raise FileNotFoundError("names.dmp not found in taxdump archive")

        names_fh = tf.extractfile(names_member)
        if names_fh is None:
            raise FileNotFoundError("Could not extract names.dmp")

        reader = io.TextIOWrapper(names_fh, encoding="utf-8", errors="replace")
        for line in tqdm(reader, desc="names.dmp", unit=" lines"):
            fields = _parse_dmp_line(line)
            if len(fields) < 4:
                continue
            try:
                taxid = int(fields[0])
            except ValueError:
                continue

            if taxid not in taxid_ranks:
                continue

            name = fields[1].strip()
            name_class = fields[3].strip().lower()

            entry = taxid_names.setdefault(taxid, {"scientific": "", "common": []})

            if name_class == "scientific name":
                entry["scientific"] = name
            elif name_class in ("common name", "genbank common name"):
                common_list = entry["common"]
                assert isinstance(common_list, list)
                common_list.append(name)

    # -----------------------------------------------------------------------
    # Step 3: Write JSONL
    # -----------------------------------------------------------------------
    total_written = 0

    with open(output_path, "w", encoding="utf-8") as out_fh:
        for taxid, rank in tqdm(taxid_ranks.items(), desc="Writing NCBI JSONL", unit=" taxa"):
            names = taxid_names.get(taxid)
            if names is None or not names["scientific"]:
                continue

            canonical = str(names["scientific"])
            labels: dict[str, str] = {}
            common_list = names["common"]
            assert isinstance(common_list, list)
            if common_list:
                labels["en"] = common_list[0]
                for i, cn in enumerate(common_list[1:], start=1):
                    labels[f"en_{i}"] = cn

            record = {
                "taxid": taxid,
                "name": canonical,
                "labels": labels,
                "rank": rank,
                "category": "taxon",
            }
            out_fh.write(json.dumps(record, ensure_ascii=False) + "\n")
            total_written += 1

    logger.info("NCBI parse complete: %d concepts written to %s", total_written, output_path)
    return total_written


if __name__ == "__main__":
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )
    from config import CHECKPOINT_DIR, RAW_DIR

    RAW_DIR.mkdir(parents=True, exist_ok=True)
    CHECKPOINT_DIR.mkdir(parents=True, exist_ok=True)
    fetch_all(RAW_DIR / "ncbi_taxonomy.jsonl", CHECKPOINT_DIR)
