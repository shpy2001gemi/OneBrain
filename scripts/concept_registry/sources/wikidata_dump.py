"""
Wikidata JSON dump STREAMING parser for OneBrain Concept Registry.

Streams the Wikidata dump (~102GB bz2) directly from HTTP without
saving to disk. Decompresses on-the-fly and parses entities,
filtering for CONCEPTS only.

Features:
  - Auto-retry on connection errors (max 50 retries)
  - Resume via checkpoint (re-downloads from byte 0 but skips seen QIDs)
  - Checkpoint every 100K items
"""

import bz2
import json
import logging
import sys
import time
from pathlib import Path
from typing import Optional

import requests
from tqdm import tqdm

logger = logging.getLogger(__name__)

# Dated dump URL (updated weekly)
DUMP_URL = "https://dumps.wikimedia.org/wikidatawiki/entities/20260706/wikidata-20260706-all.json.bz2"

# Languages to extract labels for
LABEL_LANGUAGES = ["en", "vi", "fr", "de", "es", "ja", "zh", "ko"]

# Cross-reference properties to extract
CROSS_REF_PROPS = {
    "P683": "chebi",     # ChEBI ID
    "P846": "ncbi",      # NCBI taxonomy ID
    "P1566": "geonames", # GeoNames ID
    "P231": "cas",       # CAS Registry Number
}

# P31 values to EXCLUDE (specific instances, not concepts)
EXCLUDE_P31: set[int] = {
    # --- People ---
    5,          # Q5: human

    # --- Specific places (GeoNames covers these) ---
    515,        # Q515: city
    6256,       # Q6256: country
    532,        # Q532: village
    23442,      # Q23442: island
    8502,       # Q8502: mountain
    4022,       # Q4022: river
    23397,      # Q23397: lake
    82794,      # Q82794: region (geographic)
    3957,       # Q3957: town
    5119,       # Q5119: capital city
    1549591,    # Q1549591: big city
    486972,     # Q486972: human settlement
    15284,      # Q15284: municipality
    262166,     # Q262166: municipality of Germany
    1093829,    # Q1093829: city of the US
    253019,     # Q253019: commune of France

    # --- Media instances ---
    11424,      # Q11424: film
    24856,      # Q24856: TV series
    5398426,    # Q5398426: TV episode
    506240,     # Q506240: TV season
    7889,       # Q7889: video game

    # --- Publication instances ---
    13442814,   # Q13442814: scholarly article
    571,        # Q571: book (specific book instance)
    7725634,    # Q7725634: literary work
    49848,      # Q49848: document
    1002697,    # Q1002697: periodical
    5633421,    # Q5633421: scientific journal
    685935,     # Q685935: trade magazine

    # --- Specific organisms (NCBI covers) ---
    16521,      # Q16521: taxon

    # --- Business/organization instances ---
    4830453,    # Q4830453: business
    43229,      # Q43229: organization (specific)
    3918,       # Q3918: university (specific institution)
    3914,       # Q3914: school (specific)
    476028,     # Q476028: political party (specific)
    18388277,   # Q18388277: football club

    # --- Specific astronomical objects ---
    523,        # Q523: star (specific star)
    318,        # Q318: galaxy (specific galaxy)
    3863,       # Q3863: asteroid

    # --- Wikimedia meta pages ---
    22808320,   # Q22808320: Wikimedia human name disambiguation
    4167836,    # Q4167836: Wikimedia category
    13406463,   # Q13406463: Wikimedia list
    17362920,   # Q17362920: Wikimedia template
    11266439,   # Q11266439: Wikimedia template
    4167410,    # Q4167410: Wikimedia disambiguation page
}

# Minimum sitelinks to consider an item "notable"
MIN_SITELINKS = 2

# Progress checkpoint interval
CHECKPOINT_INTERVAL = 100_000

# Retry config
MAX_RETRIES = 50
RETRY_WAIT_BASE = 30  # seconds, doubles each retry (capped at 300s)

HEADERS = {"User-Agent": "OneBrain/1.0 (concept-registry; contact@onebrain.org)"}


def _extract_p31_qids(claims: dict) -> set[int]:
    """Extract numeric QIDs from P31 (instance_of) claims."""
    qids = set()
    for claim in claims.get("P31", []):
        try:
            qid = claim["mainsnak"]["datavalue"]["value"]["numeric-id"]
            qids.add(qid)
        except (KeyError, TypeError):
            pass
    return qids


def _extract_cross_refs(claims: dict) -> dict[str, str]:
    """Extract cross-reference property values."""
    refs = {}
    for prop, label in CROSS_REF_PROPS.items():
        for claim in claims.get(prop, []):
            try:
                val = claim["mainsnak"]["datavalue"]["value"]
                if isinstance(val, str):
                    refs[prop] = val
                break
            except (KeyError, TypeError):
                pass
    return refs


def _extract_labels(entity: dict) -> dict[str, str]:
    """Extract labels in target languages."""
    labels = {}
    entity_labels = entity.get("labels", {})
    for lang in LABEL_LANGUAGES:
        if lang in entity_labels:
            labels[lang] = entity_labels[lang].get("value", "")
    return labels


def _is_concept(p31_qids: set[int]) -> bool:
    """Check if item is a concept (not an excluded instance type)."""
    if not p31_qids:
        return True
    return p31_qids.isdisjoint(EXCLUDE_P31)


class BZ2StreamDecoder:
    """Incrementally decompress bz2 data from a byte stream."""

    def __init__(self):
        self._decompressor = bz2.BZ2Decompressor()
        self._buffer = b""

    def feed(self, chunk: bytes) -> list[str]:
        """Feed compressed bytes, return complete lines."""
        lines = []

        while chunk:
            try:
                decompressed = self._decompressor.decompress(chunk)
                self._buffer += decompressed
                chunk = b""

                if self._decompressor.eof:
                    unused = self._decompressor.unused_data
                    self._decompressor = bz2.BZ2Decompressor()
                    chunk = unused

            except EOFError:
                break

        while b"\n" in self._buffer:
            line, self._buffer = self._buffer.split(b"\n", 1)
            try:
                lines.append(line.decode("utf-8"))
            except UnicodeDecodeError:
                pass

        return lines


def _process_entity(entity: dict, seen_qids: set, stats: dict) -> dict | None:
    """Process a single entity. Returns record dict or None if filtered."""
    entity_id = entity.get("id", "")
    if not entity_id.startswith("Q"):
        stats["non_q"] += 1
        return None

    qid = int(entity_id[1:])
    if qid in seen_qids:
        return None

    claims = entity.get("claims", {})
    p31_qids = _extract_p31_qids(claims)

    if not _is_concept(p31_qids):
        stats["excluded_p31"] += 1
        return None

    sitelinks = len(entity.get("sitelinks", {}))
    if sitelinks < MIN_SITELINKS:
        stats["excluded_sitelinks"] += 1
        return None

    labels = _extract_labels(entity)
    if not labels.get("en"):
        stats["excluded_no_label"] += 1
        return None

    descriptions = entity.get("descriptions", {})
    description = descriptions.get("en", {}).get("value", "")
    cross_refs = _extract_cross_refs(claims)

    return {
        "qid": qid,
        "labels": labels,
        "description": description,
        "category": "concept",
        "cross_refs": cross_refs,
        "sitelinks": sitelinks,
    }


def _stream_from_http(url: str) -> tuple:
    """Open HTTP stream. Returns (response, content_length)."""
    resp = requests.get(url, stream=True, headers=HEADERS, timeout=600)
    resp.raise_for_status()
    cl = int(resp.headers.get("Content-Length", 0))
    return resp, cl


def fetch_all(
    output_path: Path,
    checkpoint_dir: Path,
    dump_path: Optional[Path] = None,
    target_count: int = 10_000_000,
) -> int:
    """Stream-parse Wikidata dump with auto-retry on connection errors.

    On each retry, re-starts the HTTP stream from byte 0 but skips
    already-seen QIDs (kept in memory). This is reliable because
    bz2 cannot resume from arbitrary byte offsets.
    """
    checkpoint_path = checkpoint_dir / "wikidata_dump_checkpoint.json"
    output_path.parent.mkdir(parents=True, exist_ok=True)
    checkpoint_dir.mkdir(parents=True, exist_ok=True)

    # Load existing QIDs for resume
    seen_qids: set[int] = set()
    items_written = 0
    if output_path.exists():
        logger.info("Loading existing entries for resume...")
        with open(output_path, "r", encoding="utf-8") as f:
            for line in f:
                try:
                    obj = json.loads(line)
                    seen_qids.add(obj["qid"])
                except Exception:
                    pass
        items_written = len(seen_qids)
        logger.info("Loaded %d existing entries", items_written)

    if items_written >= target_count:
        logger.info("Already have %d entries (target: %d), done!", items_written, target_count)
        return items_written

    # Get source size
    use_local = dump_path and dump_path.exists()
    if use_local:
        source_size = dump_path.stat().st_size
    else:
        head_resp = requests.head(DUMP_URL, headers=HEADERS, timeout=30, allow_redirects=True)
        source_size = int(head_resp.headers.get("Content-Length", 0))
        logger.info("Dump size: %.1f GB", source_size / 1e9)

    stats = {
        "kept": items_written,
        "excluded_p31": 0,
        "excluded_sitelinks": 0,
        "excluded_no_label": 0,
        "non_q": 0,
        "parse_errors": 0,
        "retries": 0,
    }

    t0 = time.time()
    retry_count = 0

    while items_written < target_count and retry_count <= MAX_RETRIES:
        # Fresh decoder for each attempt (bz2 can't resume mid-block)
        decoder = BZ2StreamDecoder()
        total_bytes_this_attempt = 0

        try:
            if use_local:
                logger.info("Reading from local dump: %s", dump_path)
                fh = open(dump_path, "rb")
                def chunk_iter():
                    while True:
                        chunk = fh.read(1024 * 1024)
                        if not chunk:
                            break
                        yield chunk
                    fh.close()
                data_iter = chunk_iter()
            else:
                attempt_label = f" (attempt {retry_count + 1})" if retry_count > 0 else ""
                logger.info("Streaming from %s%s — have %d concepts, skipping seen QIDs",
                            DUMP_URL, attempt_label, items_written)
                resp, _ = _stream_from_http(DUMP_URL)
                data_iter = resp.iter_content(chunk_size=1024 * 1024)

            pbar_bytes = tqdm(total=source_size, unit="B", unit_scale=True,
                              desc=f"Stream (attempt {retry_count + 1})")
            pbar_items = tqdm(desc="Concepts found", unit=" items",
                              initial=items_written, total=target_count)

            with open(output_path, "a", encoding="utf-8") as out_fh:
                for chunk in data_iter:
                    total_bytes_this_attempt += len(chunk)
                    pbar_bytes.update(len(chunk))

                    lines = decoder.feed(chunk)
                    for raw_line in lines:
                        raw_line = raw_line.strip().rstrip(",")
                        if raw_line in ("[", "]", ""):
                            continue

                        try:
                            entity = json.loads(raw_line)
                        except json.JSONDecodeError:
                            stats["parse_errors"] += 1
                            continue

                        record = _process_entity(entity, seen_qids, stats)
                        if record is None:
                            continue

                        out_fh.write(json.dumps(record, ensure_ascii=False) + "\n")
                        seen_qids.add(record["qid"])
                        items_written += 1
                        stats["kept"] = items_written

                        pbar_items.update(1)
                        elapsed = time.time() - t0
                        pbar_items.set_postfix(
                            rate=f"{items_written / max(1, elapsed):.0f}/s",
                            retries=retry_count,
                        )

                        # Checkpoint
                        if items_written % CHECKPOINT_INTERVAL == 0:
                            out_fh.flush()
                            with open(checkpoint_path, "w") as cp:
                                json.dump({
                                    "items_written": items_written,
                                    "bytes_consumed": total_bytes_this_attempt,
                                    "retries": retry_count,
                                }, cp)
                            logger.info(
                                "Checkpoint: %d concepts (%.0f min, retry=%d)",
                                items_written, elapsed / 60, retry_count,
                            )

                        if items_written >= target_count:
                            break

                    if items_written >= target_count:
                        break

            pbar_bytes.close()
            pbar_items.close()

            # If we got here without error, we're done (reached target or EOF)
            break

        except (requests.exceptions.ChunkedEncodingError,
                requests.exceptions.ConnectionError,
                requests.exceptions.Timeout,
                ConnectionError,
                OSError) as e:

            try:
                pbar_bytes.close()
                pbar_items.close()
            except Exception:
                pass

            retry_count += 1
            stats["retries"] = retry_count

            # Save checkpoint before retry
            with open(checkpoint_path, "w") as cp:
                json.dump({
                    "items_written": items_written,
                    "bytes_consumed": total_bytes_this_attempt,
                    "retries": retry_count,
                }, cp)

            if retry_count > MAX_RETRIES:
                logger.error("Max retries (%d) exceeded. Stopping.", MAX_RETRIES)
                raise

            wait = min(RETRY_WAIT_BASE * (2 ** min(retry_count - 1, 4)), 300)
            logger.warning(
                "Connection error after %d concepts, %.1f GB. "
                "Retry %d/%d in %ds. Error: %s",
                items_written, total_bytes_this_attempt / 1e9,
                retry_count, MAX_RETRIES, wait, str(e)[:200],
            )
            time.sleep(wait)

    # Final checkpoint
    with open(checkpoint_path, "w") as cp:
        json.dump({
            "items_written": items_written,
            "bytes_consumed": 0,  # reset — not meaningful across retries
            "retries": retry_count,
        }, cp)

    elapsed = time.time() - t0
    logger.info("Wikidata stream parse complete (%.0f min):", elapsed / 60)
    logger.info("  Concepts kept:       %d", stats["kept"])
    logger.info("  Excluded (P31):      %d", stats["excluded_p31"])
    logger.info("  Excluded (sitelinks):%d", stats["excluded_sitelinks"])
    logger.info("  Excluded (no label): %d", stats["excluded_no_label"])
    logger.info("  Non-Q items:         %d", stats["non_q"])
    logger.info("  Parse errors:        %d", stats["parse_errors"])
    logger.info("  Connection retries:  %d", stats["retries"])

    return items_written


if __name__ == "__main__":
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )
    from config import CHECKPOINT_DIR, RAW_DIR

    RAW_DIR.mkdir(parents=True, exist_ok=True)
    CHECKPOINT_DIR.mkdir(parents=True, exist_ok=True)

    fetch_all(
        RAW_DIR / "wikidata.jsonl",
        CHECKPOINT_DIR,
        target_count=10_000_000,
    )
