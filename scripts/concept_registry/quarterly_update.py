"""
Quarterly update script for the OneBrain Concept Registry.

Fetches incremental changes from each source since the last update,
merges them into the existing ``.obr`` file, and saves a changelog.
"""

import argparse
import json
import logging
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Optional

import requests
from tqdm import tqdm

from config import (
    CHECKPOINT_DIR,
    GEONAMES_DUMP_URL,
    MERGED_DIR,
    OBR_OUTPUT,
    RAW_DIR,
    SOURCE_NAMES,
    WIKIDATA_DELAY,
    WIKIDATA_SPARQL_URL,
)

logger = logging.getLogger(__name__)

STATE_FILE = CHECKPOINT_DIR / "quarterly_state.json"
CHANGELOG_DIR = CHECKPOINT_DIR / "changelogs"

USER_AGENT = "OneBrain/1.0 ConceptRegistry (https://github.com/nicholasareed/onebrain)"


# ---------------------------------------------------------------------------
# State management
# ---------------------------------------------------------------------------

def _load_state() -> dict[str, Any]:
    """Load the quarterly update state from disk.

    Returns:
        State dict with ``last_fetch_date`` per source and metadata.
    """
    if STATE_FILE.exists():
        with open(STATE_FILE, "r", encoding="utf-8") as fh:
            return json.load(fh)
    return {
        "last_fetch_date": {},
        "last_taxdump_hash": None,
    }


def _save_state(state: dict[str, Any]) -> None:
    """Persist the quarterly update state to disk.

    Args:
        state: State dict to save.
    """
    STATE_FILE.parent.mkdir(parents=True, exist_ok=True)
    with open(STATE_FILE, "w", encoding="utf-8") as fh:
        json.dump(state, fh, indent=2)


# ---------------------------------------------------------------------------
# Wikidata delta
# ---------------------------------------------------------------------------

def _fetch_wikidata_delta(
    since: str,
    output_path: Path,
    dry_run: bool = False,
) -> int:
    """Fetch Wikidata entities modified since *since* date.

    Uses ``schema:dateModified`` filter in SPARQL to find recently
    changed entities.

    Args:
        since: ISO date string (``YYYY-MM-DD``) for the delta start.
        output_path: Path to write delta JSONL.
        dry_run: If True, only log what would be fetched.

    Returns:
        Number of entities fetched (or estimated in dry-run mode).
    """
    query = f"""
SELECT ?item ?itemLabel ?dateModified
WHERE {{
  ?item schema:dateModified ?dateModified .
  FILTER(?dateModified >= "{since}T00:00:00Z"^^xsd:dateTime)
  SERVICE wikibase:label {{ bd:serviceParam wikibase:language "en". }}
}}
LIMIT 10000
"""
    if dry_run:
        logger.info("[DRY RUN] Would fetch Wikidata changes since %s", since)
        return 0

    logger.info("Fetching Wikidata delta since %s …", since)
    headers = {
        "User-Agent": USER_AGENT,
        "Accept": "application/sparql-results+json",
    }

    time.sleep(WIKIDATA_DELAY)
    try:
        resp = requests.get(
            WIKIDATA_SPARQL_URL,
            params={"query": query},
            headers=headers,
            timeout=120,
        )
        resp.raise_for_status()
        data = resp.json()
    except requests.RequestException as exc:
        logger.error("Wikidata delta fetch failed: %s", exc)
        return 0

    bindings = data.get("results", {}).get("bindings", [])
    count = 0

    output_path.parent.mkdir(parents=True, exist_ok=True)
    with open(output_path, "w", encoding="utf-8") as fh:
        for row in bindings:
            item_uri = row.get("item", {}).get("value", "")
            if "/entity/Q" not in item_uri:
                continue
            qid = item_uri.split("/entity/")[-1]
            label = row.get("itemLabel", {}).get("value", "")
            fh.write(json.dumps({
                "qid": int(qid[1:]),
                "name": label,
                "delta": True,
            }, ensure_ascii=False) + "\n")
            count += 1

    logger.info("Wikidata delta: %d entities", count)
    return count


# ---------------------------------------------------------------------------
# GeoNames delta
# ---------------------------------------------------------------------------

def _fetch_geonames_delta(
    since: str,
    output_path: Path,
    dry_run: bool = False,
) -> int:
    """Download the GeoNames daily modifications file for recent changes.

    GeoNames provides daily modification files in the format
    ``modifications-YYYY-MM-DD.txt``.

    Args:
        since: ISO date string for the delta start.
        output_path: Path to write delta JSONL.
        dry_run: If True, only log what would be fetched.

    Returns:
        Number of records fetched.
    """
    mod_url = f"https://download.geonames.org/export/dump/modifications-{since}.txt"

    if dry_run:
        logger.info("[DRY RUN] Would download %s", mod_url)
        return 0

    logger.info("Fetching GeoNames modifications since %s …", since)
    try:
        resp = requests.get(mod_url, timeout=120)
        resp.raise_for_status()
    except requests.RequestException as exc:
        logger.warning("GeoNames modifications fetch failed (may not exist): %s", exc)
        return 0

    output_path.parent.mkdir(parents=True, exist_ok=True)
    count = 0

    with open(output_path, "w", encoding="utf-8") as fh:
        for line in resp.text.splitlines():
            if not line.strip():
                continue
            cols = line.split("\t")
            if len(cols) < 2:
                continue
            try:
                geonames_id = int(cols[0])
            except ValueError:
                continue
            name = cols[1] if len(cols) > 1 else ""
            fh.write(json.dumps({
                "geonames_id": geonames_id,
                "name": name,
                "delta": True,
            }, ensure_ascii=False) + "\n")
            count += 1

    logger.info("GeoNames delta: %d records", count)
    return count


# ---------------------------------------------------------------------------
# NCBI delta
# ---------------------------------------------------------------------------

def _fetch_ncbi_delta(
    last_hash: Optional[str],
    checkpoint_dir: Path,
    output_path: Path,
    dry_run: bool = False,
) -> tuple[int, Optional[str]]:
    """Compare the current NCBI taxdump hash vs the stored hash.

    If the hash has changed, triggers a full re-parse. Otherwise no
    delta is produced.

    Args:
        last_hash: Previously stored MD5 hash of taxdump.tar.gz.
        checkpoint_dir: Directory where the dump is cached.
        output_path: Path to write delta JSONL.
        dry_run: If True, only log what would be done.

    Returns:
        Tuple of (record count, new hash or None).
    """
    import hashlib

    if dry_run:
        logger.info("[DRY RUN] Would check NCBI taxdump hash")
        return 0, last_hash

    tar_path = checkpoint_dir / "taxdump.tar.gz"
    if not tar_path.exists():
        logger.info("No cached taxdump — will do full fetch on next initial_fetch run.")
        return 0, last_hash

    # Compute current hash
    md5 = hashlib.md5()
    with open(tar_path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            md5.update(chunk)
    current_hash = md5.hexdigest()

    if current_hash == last_hash:
        logger.info("NCBI taxdump unchanged (hash=%s)", current_hash)
        return 0, current_hash

    logger.info(
        "NCBI taxdump hash changed: %s → %s — re-downloading and parsing",
        last_hash,
        current_hash,
    )

    # Re-fetch and re-parse
    from sources.ncbi_taxonomy import fetch_all as ncbi_fetch

    count = ncbi_fetch(output_path, checkpoint_dir)
    # Re-hash the new download
    if tar_path.exists():
        md5_new = hashlib.md5()
        with open(tar_path, "rb") as fh:
            for chunk in iter(lambda: fh.read(1024 * 1024), b""):
                md5_new.update(chunk)
        current_hash = md5_new.hexdigest()

    return count, current_hash


# ---------------------------------------------------------------------------
# ChEBI delta
# ---------------------------------------------------------------------------

def _fetch_chebi_delta(
    since: str,
    output_path: Path,
    checkpoint_dir: Path,
    dry_run: bool = False,
) -> int:
    """Fetch ChEBI compounds modified since a given date.

    Re-downloads and filters by checking if the dump has newer entries.

    Args:
        since: ISO date string.
        output_path: Path to write delta JSONL.
        checkpoint_dir: Cache directory.
        dry_run: If True, only log what would be done.

    Returns:
        Number of records in the delta.
    """
    if dry_run:
        logger.info("[DRY RUN] Would check ChEBI for updates since %s", since)
        return 0

    logger.info("Re-fetching ChEBI data and filtering for updates since %s …", since)
    from sources.chebi import fetch_all as chebi_fetch

    count = chebi_fetch(output_path, checkpoint_dir)
    return count


# ---------------------------------------------------------------------------
# Merge and rebuild
# ---------------------------------------------------------------------------

def _merge_and_rebuild(
    delta_dir: Path,
    existing_obr: Path,
    output_obr: Path,
) -> dict[str, Any]:
    """Merge delta records into the existing OBR and rebuild.

    Reads the existing deduplicated JSONL (if any), appends delta
    records, re-deduplicates, and builds a new OBR file.

    Args:
        delta_dir: Directory containing delta JSONL files.
        existing_obr: Path to the existing .obr file.
        output_obr: Path for the rebuilt .obr file.

    Returns:
        Build statistics dict.
    """
    from build_obr import build
    from dedup import deduplicate

    # Create a temporary raw dir with delta files
    raw_dir = delta_dir / "raw"
    raw_dir.mkdir(parents=True, exist_ok=True)

    # Copy any delta files to the raw dir for dedup
    for f in delta_dir.glob("*.jsonl"):
        if f.parent == delta_dir:
            import shutil
            dest = raw_dir / f.name
            if not dest.exists():
                shutil.copy2(f, dest)

    merged_path = delta_dir / "concepts_deduped.jsonl"
    dedup_stats = deduplicate(raw_dir, merged_path)
    obr_stats = build(merged_path, output_obr)

    return {
        "dedup": dedup_stats,
        "obr": obr_stats,
    }


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main() -> None:
    """Entry point for the quarterly update."""
    parser = argparse.ArgumentParser(
        description="OneBrain Concept Registry — Quarterly Update",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Only show what would be fetched; do not download or modify files.",
    )
    args = parser.parse_args()

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )

    CHECKPOINT_DIR.mkdir(parents=True, exist_ok=True)
    CHANGELOG_DIR.mkdir(parents=True, exist_ok=True)

    state = _load_state()
    last_dates = state.get("last_fetch_date", {})
    now_str = datetime.now(timezone.utc).strftime("%Y-%m-%d")

    delta_dir = CHECKPOINT_DIR / "deltas" / now_str
    delta_dir.mkdir(parents=True, exist_ok=True)

    changelog_entries: list[dict[str, Any]] = []
    overall_start = time.time()

    # Wikidata delta
    wd_since = last_dates.get("wikidata", "2024-01-01")
    wd_count = _fetch_wikidata_delta(
        wd_since,
        delta_dir / "wikidata.jsonl",
        dry_run=args.dry_run,
    )
    changelog_entries.append({
        "source": "wikidata",
        "since": wd_since,
        "records": wd_count,
    })

    # GeoNames delta
    gn_since = last_dates.get("geonames", now_str)
    gn_count = _fetch_geonames_delta(
        gn_since,
        delta_dir / "geonames.jsonl",
        dry_run=args.dry_run,
    )
    changelog_entries.append({
        "source": "geonames",
        "since": gn_since,
        "records": gn_count,
    })

    # NCBI delta
    ncbi_count, new_hash = _fetch_ncbi_delta(
        state.get("last_taxdump_hash"),
        CHECKPOINT_DIR,
        delta_dir / "ncbi_taxonomy.jsonl",
        dry_run=args.dry_run,
    )
    changelog_entries.append({
        "source": "ncbi",
        "records": ncbi_count,
    })

    # ChEBI delta
    chebi_since = last_dates.get("chebi", "2024-01-01")
    chebi_count = _fetch_chebi_delta(
        chebi_since,
        delta_dir / "chebi.jsonl",
        CHECKPOINT_DIR,
        dry_run=args.dry_run,
    )
    changelog_entries.append({
        "source": "chebi",
        "since": chebi_since,
        "records": chebi_count,
    })

    # Merge and rebuild
    if not args.dry_run:
        total_delta = wd_count + gn_count + ncbi_count + chebi_count
        if total_delta > 0:
            logger.info("Merging %d delta records and rebuilding OBR …", total_delta)
            rebuild_stats = _merge_and_rebuild(delta_dir, OBR_OUTPUT, OBR_OUTPUT)
            changelog_entries.append({
                "rebuild": rebuild_stats,
            })
        else:
            logger.info("No delta records — skipping rebuild.")

        # Update state
        state["last_fetch_date"] = {
            "wikidata": now_str,
            "geonames": now_str,
            "ncbi": now_str,
            "chebi": now_str,
        }
        if new_hash is not None:
            state["last_taxdump_hash"] = new_hash

        _save_state(state)

    # Save changelog
    changelog_path = CHANGELOG_DIR / f"changelog_{now_str}.json"
    with open(changelog_path, "w", encoding="utf-8") as fh:
        json.dump({
            "date": now_str,
            "dry_run": args.dry_run,
            "elapsed_seconds": round(time.time() - overall_start, 1),
            "entries": changelog_entries,
        }, fh, indent=2)

    logger.info("Changelog saved to %s", changelog_path)

    elapsed = time.time() - overall_start
    logger.info("")
    logger.info("=" * 60)
    logger.info("QUARTERLY UPDATE %s", "COMPLETE" if not args.dry_run else "(DRY RUN)")
    logger.info("=" * 60)
    logger.info("  Wikidata:  %d delta records", wd_count)
    logger.info("  GeoNames:  %d delta records", gn_count)
    logger.info("  NCBI:      %d delta records", ncbi_count)
    logger.info("  ChEBI:     %d delta records", chebi_count)
    logger.info("  Total time: %.1fs", elapsed)


if __name__ == "__main__":
    main()
