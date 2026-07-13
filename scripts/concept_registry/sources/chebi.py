"""
ChEBI dump parser for the OneBrain Concept Registry.

Downloads ``compounds.sql.zip`` and ``names.sql.zip`` from the EBI FTP
(ChEBI 2.0 generic_dump_allstar) and parses PostgreSQL INSERT statements
to produce substance concept records as JSONL.
"""

import gzip
import io
import json
import logging
import re
import zipfile
from pathlib import Path
from typing import Optional

import requests
from tqdm import tqdm

from config import CATEGORY_SUBSTANCE, CHEBI_COMPOUNDS_URL, CHEBI_NAMES_URL

logger = logging.getLogger(__name__)

# Minimum star rating to include a compound
MIN_STAR_RATING = 2
ACCEPTED_STATUS_ID = 3  # Checked (status_id: 1=Submitted, 2=Waiting, 3=Checked)

MAX_SYNONYMS = 20  # Limit synonyms per compound

# Regex to extract the VALUES tuple from a PostgreSQL INSERT statement.
# Captures everything inside VALUES (...); Supports schema-qualified names (e.g. public.compounds)
_INSERT_RE = re.compile(r"INSERT\s+INTO\s+[\w.]+\s*\([^)]*\)\s*VALUES\s*\((.+)\)\s*;", re.IGNORECASE)


def _download_file(url: str, dest_path: Path, label: str) -> None:
    """Download a file with progress bar.

    Args:
        url: Source URL.
        dest_path: Destination path.
        label: Human-readable label for the progress bar.
    """
    logger.info("Downloading %s from %s", label, url)
    resp = requests.get(url, stream=True, timeout=600)
    resp.raise_for_status()

    total_size = int(resp.headers.get("content-length", 0))
    dest_path.parent.mkdir(parents=True, exist_ok=True)

    with (
        open(dest_path, "wb") as fh,
        tqdm(
            total=total_size,
            unit="B",
            unit_scale=True,
            desc=label,
        ) as pbar,
    ):
        for chunk in resp.iter_content(chunk_size=1024 * 1024):
            fh.write(chunk)
            pbar.update(len(chunk))

    logger.info("%s saved to %s", label, dest_path)


def _parse_sql_values(values_str: str) -> list[Optional[str]]:
    """Parse a comma-separated SQL VALUES string into a list of Python values.

    Handles:
      - NULL → None
      - Quoted strings 'abc' → "abc" (with '' escape → ')
      - Unquoted literals (numbers, etc.) → as-is string

    Args:
        values_str: The content between VALUES( ... ) in an INSERT statement.

    Returns:
        List of string values (or None for NULL).
    """
    result: list[Optional[str]] = []
    i = 0
    n = len(values_str)

    while i < n:
        # skip whitespace
        while i < n and values_str[i] in (" ", "\t"):
            i += 1

        if i >= n:
            break

        if values_str[i] == "'":
            # Quoted string – scan until unescaped closing quote
            i += 1  # skip opening quote
            parts: list[str] = []
            while i < n:
                if values_str[i] == "'" and i + 1 < n and values_str[i + 1] == "'":
                    parts.append("'")
                    i += 2
                elif values_str[i] == "'":
                    i += 1  # skip closing quote
                    break
                else:
                    parts.append(values_str[i])
                    i += 1
            result.append("".join(parts))
        else:
            # Unquoted token (number or NULL)
            start = i
            while i < n and values_str[i] not in (",", ")"):
                i += 1
            token = values_str[start:i].strip()
            if token.upper() == "NULL":
                result.append(None)
            else:
                result.append(token)

        # skip comma separator
        while i < n and values_str[i] in (" ", "\t"):
            i += 1
        if i < n and values_str[i] == ",":
            i += 1

    return result


def _read_sql_from_zip(zip_path: Path) -> str:
    """Read the SQL content from a .sql.zip (or .sql.gz) archive.

    ChEBI 2.0 serves files with .sql.zip extension but they may actually
    be gzip-compressed. This function tries zipfile first, then falls
    back to gzip.

    Args:
        zip_path: Path to the archive file.

    Returns:
        The contents of the SQL file as a string.
    """
    # Try as zip first
    try:
        with zipfile.ZipFile(zip_path, "r") as zf:
            sql_files = [n for n in zf.namelist() if n.endswith(".sql")]
            if not sql_files:
                raise FileNotFoundError(f"No .sql file found in {zip_path}")
            with zf.open(sql_files[0]) as fh:
                return io.TextIOWrapper(fh, encoding="utf-8", errors="replace").read()
    except zipfile.BadZipFile:
        logger.info("File %s is not a zip, trying gzip...", zip_path)
        with gzip.open(zip_path, "rt", encoding="utf-8", errors="replace") as fh:
            return fh.read()


def _parse_column_names(sql_text: str) -> list[str]:
    """Extract column names from the first INSERT INTO ... (...) statement.

    Args:
        sql_text: Full SQL dump text.

    Returns:
        List of column names in order.
    """
    m = re.search(r"INSERT\s+INTO\s+[\w.]+\s*\(([^)]+)\)", sql_text, re.IGNORECASE)
    if not m:
        return []
    return [c.strip().lower() for c in m.group(1).split(",")]


def fetch_all(output_path: Path, checkpoint_dir: Path) -> int:
    """Parse ChEBI compound and name SQL dumps and write JSONL output.

    Downloads the SQL zip files to *checkpoint_dir* if not already present,
    then filters by star rating and status, merging synonyms from the
    names file.

    Args:
        output_path: Path to the output JSONL file.
        checkpoint_dir: Directory used for caching the downloaded files.

    Returns:
        Total number of concepts written.
    """
    compounds_path = checkpoint_dir / "compounds.sql.zip"
    names_path = checkpoint_dir / "names.sql.zip"

    if not compounds_path.exists():
        _download_file(CHEBI_COMPOUNDS_URL, compounds_path, "ChEBI compounds")
    else:
        logger.info("ChEBI compounds already cached at %s", compounds_path)

    if not names_path.exists():
        _download_file(CHEBI_NAMES_URL, names_path, "ChEBI names")
    else:
        logger.info("ChEBI names already cached at %s", names_path)

    output_path.parent.mkdir(parents=True, exist_ok=True)

    # -----------------------------------------------------------------------
    # Step 1: Parse compounds.sql.zip to identify qualifying compounds
    # -----------------------------------------------------------------------
    logger.info("Parsing ChEBI compounds …")

    compounds_sql = _read_sql_from_zip(compounds_path)
    columns = _parse_column_names(compounds_sql)

    if not columns:
        raise ValueError("Could not parse column names from compounds.sql")

    # Build column index lookup
    col_idx = {name: i for i, name in enumerate(columns)}

    # compound_id → {"name": str, "definition": str}
    compounds: dict[int, dict[str, str]] = {}

    for match in tqdm(_INSERT_RE.finditer(compounds_sql), desc="ChEBI compounds", unit=" rows"):
        values = _parse_sql_values(match.group(1))

        # Get status_id (integer: 1=Submitted, 2=Waiting, 3=Checked)
        status_idx = col_idx.get("status_id")
        if status_idx is not None and status_idx < len(values):
            try:
                status_id = int(values[status_idx] or "0")
            except ValueError:
                continue
        else:
            continue
        if status_id != ACCEPTED_STATUS_ID:
            continue

        # Get star rating (column name: 'stars')
        star_idx = col_idx.get("stars")
        if star_idx is not None and star_idx < len(values):
            try:
                star = int(values[star_idx] or "0")
            except ValueError:
                star = 0
        else:
            star = 0
        if star < MIN_STAR_RATING:
            continue

        # Get compound ID
        id_idx = col_idx.get("id")
        if id_idx is not None and id_idx < len(values):
            try:
                chebi_id = int(values[id_idx] or "0")
            except ValueError:
                continue
        else:
            continue

        # Get name
        name_idx = col_idx.get("name")
        name = ""
        if name_idx is not None and name_idx < len(values):
            name = (values[name_idx] or "").strip()

        # Get definition
        def_idx = col_idx.get("definition")
        definition = ""
        if def_idx is not None and def_idx < len(values):
            definition = (values[def_idx] or "").strip()

        compounds[chebi_id] = {
            "name": name,
            "definition": definition,
        }

    # Free memory
    del compounds_sql

    logger.info("Found %d qualifying compounds (STARS>=%d, STATUS_ID=%d)",
                len(compounds), MIN_STAR_RATING, ACCEPTED_STATUS_ID)

    # -----------------------------------------------------------------------
    # Step 2: Parse names.sql.zip for synonyms
    # -----------------------------------------------------------------------
    logger.info("Parsing ChEBI names …")

    names_sql = _read_sql_from_zip(names_path)
    name_columns = _parse_column_names(names_sql)

    if not name_columns:
        raise ValueError("Could not parse column names from names.sql")

    name_col_idx = {name: i for i, name in enumerate(name_columns)}

    # compound_id → {"synonyms": [str], "cas": str|None}
    names_data: dict[int, dict[str, object]] = {}

    for match in tqdm(_INSERT_RE.finditer(names_sql), desc="ChEBI names", unit=" rows"):
        values = _parse_sql_values(match.group(1))

        # Get compound_id
        cid_idx = name_col_idx.get("compound_id")
        if cid_idx is not None and cid_idx < len(values):
            try:
                compound_id = int(values[cid_idx] or "0")
            except ValueError:
                continue
        else:
            continue

        if compound_id not in compounds:
            continue

        # Get name
        n_idx = name_col_idx.get("name")
        syn_name = ""
        if n_idx is not None and n_idx < len(values):
            syn_name = (values[n_idx] or "").strip()

        # Get type
        t_idx = name_col_idx.get("type")
        name_type = ""
        if t_idx is not None and t_idx < len(values):
            name_type = (values[t_idx] or "").strip()

        entry = names_data.setdefault(compound_id, {"synonyms": [], "cas": None})

        if name_type == "CAS REGISTRY NUMBER" and syn_name:
            entry["cas"] = syn_name
        elif syn_name:
            syn_list = entry["synonyms"]
            assert isinstance(syn_list, list)
            if len(syn_list) < MAX_SYNONYMS:
                syn_list.append(syn_name)

    # Free memory
    del names_sql

    # -----------------------------------------------------------------------
    # Step 3: Write JSONL
    # -----------------------------------------------------------------------
    total_written = 0

    with open(output_path, "w", encoding="utf-8") as out_fh:
        for chebi_id, comp in tqdm(compounds.items(), desc="Writing ChEBI JSONL", unit=" entries"):
            extra = names_data.get(chebi_id, {"synonyms": [], "cas": None})

            labels: dict[str, str] = {}
            syn_list = extra["synonyms"]
            assert isinstance(syn_list, list)
            for i, syn in enumerate(syn_list):
                labels[f"syn_{i}"] = syn

            cas_value = extra.get("cas")
            record = {
                "chebi_id": chebi_id,
                "name": comp["name"],
                "labels": labels,
                "cas": str(cas_value) if cas_value else None,
                "category": "substance",
            }
            out_fh.write(json.dumps(record, ensure_ascii=False) + "\n")
            total_written += 1

    logger.info("ChEBI parse complete: %d concepts written to %s", total_written, output_path)
    return total_written


if __name__ == "__main__":
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )
    from config import CHECKPOINT_DIR, RAW_DIR

    RAW_DIR.mkdir(parents=True, exist_ok=True)
    CHECKPOINT_DIR.mkdir(parents=True, exist_ok=True)
    fetch_all(RAW_DIR / "chebi.jsonl", CHECKPOINT_DIR)
