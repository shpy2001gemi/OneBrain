"""
Wikidata SPARQL fetcher for the OneBrain Concept Registry.

Strategy: Fetch top-N concepts per P31 category, ordered by sitelinks
(Wikipedia article count = popularity proxy). This ensures we get the
most notable concepts, not every obscure entity.

Also includes a general "top by sitelinks" pass to catch popular items
that don't fit neatly into specific P31 categories.
"""

import json
import logging
import time
from pathlib import Path
from typing import Any, Optional

import requests
from tqdm import tqdm

from config import (
    BATCH_SIZE,
    CROSS_REF_PROPERTIES,
    LABEL_LANGUAGES,
    WIKIDATA_DELAY,
    WIKIDATA_SPARQL_URL,
)

logger = logging.getLogger(__name__)

USER_AGENT = "OneBrain/1.0 ConceptRegistry (https://github.com/nicholasareed/onebrain)"
MAX_RETRIES = 3
LABEL_BATCH_SIZE = 200  # QIDs per label-fetch query

# ---------------------------------------------------------------------------
# Category caps: max items to fetch per P31 type
# Total target: ~5M concepts from Wikidata
# ---------------------------------------------------------------------------
CATEGORY_CAPS: dict[str, tuple[str, int]] = {
    # cat_label: (P31 QID, max_items)
    # --- Large categories (cap heavily) ---
    "human":              ("Q5",       50_000),   # Only top 50K famous people
    "taxon":              ("Q16521",   200_000),  # Top 200K species/genera
    "chemical_compound":  ("Q11173",   100_000),  # Top 100K compounds

    # --- Geographic (important for gn: cross-ref) ---
    "city":               ("Q515",     200_000),
    "country":            ("Q6256",    10_000),
    "region":             ("Q82794",   50_000),
    "village":            ("Q532",     100_000),
    "island":             ("Q23442",   30_000),
    "mountain":           ("Q8502",    20_000),
    "river":              ("Q4022",    30_000),
    "lake":               ("Q23397",   15_000),

    # --- Medium categories ---
    "organization":       ("Q43229",   100_000),
    "disease":            ("Q12136",   30_000),
    "protein":            ("Q8054",    50_000),
    "gene":               ("Q7187",    50_000),
    "language":           ("Q34",      10_000),
    "occupation":         ("Q12737",   5_000),
    "sport":              ("Q349",     2_000),

    # --- Small but valuable ---
    "unit":               ("Q47574",   5_000),
    "film":               ("Q11424",   100_000),
    "book":               ("Q571",     50_000),
    "software":           ("Q7397",    30_000),
    "occurrence":         ("Q1190554", 50_000),   # Events
    "food":               ("Q2095",    20_000),
    "mineral":            ("Q7946",    5_000),
    "musical_instrument": ("Q34379",   2_000),
    "currency":           ("Q8142",    1_000),
    "astronomical_object":("Q6999",    20_000),
}

# Wikidata properties (P-items) — separate fetch, not P31-based
FETCH_PROPERTIES = True
PROPERTY_CAP = 15_000

# General "top sitelinks" pass — catch anything popular we missed
GENERAL_TOP_CAP = 500_000


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _sparql_request(query: str, *, retries: int = MAX_RETRIES) -> dict[str, Any]:
    """Execute a SPARQL query against the Wikidata Query Service.

    Includes retry logic with exponential back-off and rate limiting.
    """
    headers = {
        "User-Agent": USER_AGENT,
        "Accept": "application/sparql-results+json",
    }
    last_exc: Optional[Exception] = None

    for attempt in range(retries):
        try:
            time.sleep(WIKIDATA_DELAY)
            resp = requests.get(
                WIKIDATA_SPARQL_URL,
                params={"query": query},
                headers=headers,
                timeout=120,
            )
            resp.raise_for_status()
            return resp.json()
        except (requests.HTTPError, requests.ConnectionError, requests.Timeout) as exc:
            last_exc = exc
            wait = min(2 ** (attempt + 2), 60)  # 4s, 8s, 16s...
            logger.warning(
                "SPARQL request failed (attempt %d/%d): %s – retrying in %ds",
                attempt + 1, retries, exc, wait,
            )
            time.sleep(wait)

    raise requests.HTTPError(
        f"SPARQL request failed after {retries} retries: {last_exc}"
    ) from last_exc


def _build_category_query(p31_qid: str, limit: int, offset: int) -> str:
    """SPARQL query: fetch top items by sitelinks for a P31 type."""
    cross_ref_optionals = "\n".join(
        f"  OPTIONAL {{ ?item wdt:{prop} ?{prop} . }}"
        for prop in CROSS_REF_PROPERTIES
    )
    return f"""
SELECT ?item ?description ?sitelinks {" ".join(f"?{p}" for p in CROSS_REF_PROPERTIES)}
WHERE {{
  ?item wdt:P31 wd:{p31_qid} .
  ?item wikibase:sitelinks ?sitelinks .
  OPTIONAL {{ ?item schema:description ?description . FILTER(LANG(?description) = "en") }}
{cross_ref_optionals}
}}
ORDER BY DESC(?sitelinks)
LIMIT {limit}
OFFSET {offset}
"""


def _build_general_top_query(limit: int, offset: int) -> str:
    """SPARQL query: fetch top items by sitelinks (any type), min 5 sitelinks."""
    cross_ref_optionals = "\n".join(
        f"  OPTIONAL {{ ?item wdt:{prop} ?{prop} . }}"
        for prop in CROSS_REF_PROPERTIES
    )
    return f"""
SELECT ?item ?description ?sitelinks {" ".join(f"?{p}" for p in CROSS_REF_PROPERTIES)}
WHERE {{
  ?item wikibase:sitelinks ?sitelinks .
  FILTER(?sitelinks >= 5)
  OPTIONAL {{ ?item schema:description ?description . FILTER(LANG(?description) = "en") }}
{cross_ref_optionals}
}}
ORDER BY DESC(?sitelinks)
LIMIT {limit}
OFFSET {offset}
"""


def _build_property_query(limit: int, offset: int) -> str:
    """SPARQL query: fetch Wikidata properties (P-items)."""
    return f"""
SELECT ?item ?description
WHERE {{
  ?item a wikibase:Property .
  OPTIONAL {{ ?item schema:description ?description . FILTER(LANG(?description) = "en") }}
}}
LIMIT {limit}
OFFSET {offset}
"""


def _build_label_query(qids: list[str]) -> str:
    """SPARQL query: fetch labels for a batch of QIDs."""
    values = " ".join(f"wd:{qid}" for qid in qids)
    lang_filter = " || ".join(f'LANG(?label) = "{lang}"' for lang in LABEL_LANGUAGES)
    return f"""
SELECT ?item ?label
WHERE {{
  VALUES ?item {{ {values} }}
  ?item rdfs:label ?label .
  FILTER({lang_filter})
}}
"""


def _parse_qid(uri: str) -> Optional[str]:
    """Extract QID from a Wikidata entity URI."""
    if "/entity/" in uri:
        return uri.split("/entity/")[-1]
    return None


# ---------------------------------------------------------------------------
# Checkpoint helpers
# ---------------------------------------------------------------------------

def _load_checkpoint(checkpoint_path: Path) -> dict[str, Any]:
    """Load checkpoint state from disk."""
    if checkpoint_path.exists():
        with open(checkpoint_path, "r", encoding="utf-8") as fh:
            return json.load(fh)
    return {}


def _save_checkpoint(checkpoint_path: Path, state: dict[str, Any]) -> None:
    """Persist checkpoint state to disk."""
    checkpoint_path.parent.mkdir(parents=True, exist_ok=True)
    with open(checkpoint_path, "w", encoding="utf-8") as fh:
        json.dump(state, fh, indent=2)


# ---------------------------------------------------------------------------
# Fetch logic
# ---------------------------------------------------------------------------

def _fetch_batch_with_labels(
    qids_data: dict[str, dict[str, Any]],
    out_fh: Any,
    seen_qids: set[str],
) -> int:
    """Fetch labels for a batch of QIDs and write to output.

    Returns number of new records written (skips already-seen QIDs).
    """
    new_qids = {qid: data for qid, data in qids_data.items() if qid not in seen_qids}
    if not new_qids:
        return 0

    # Fetch labels in sub-batches
    qid_list = list(new_qids.keys())
    for i in range(0, len(qid_list), LABEL_BATCH_SIZE):
        sub_qids = qid_list[i:i + LABEL_BATCH_SIZE]
        try:
            label_query = _build_label_query(sub_qids)
            label_result = _sparql_request(label_query)
            for lrow in label_result.get("results", {}).get("bindings", []):
                qid = _parse_qid(lrow.get("item", {}).get("value", ""))
                if qid and qid in new_qids:
                    label_val = lrow.get("label", {}).get("value", "")
                    label_lang = lrow.get("label", {}).get("xml:lang", "")
                    if label_lang and label_val:
                        new_qids[qid].setdefault("labels", {})[label_lang] = label_val
        except Exception as exc:
            logger.warning("Label fetch failed for batch: %s", exc)

    # Write
    written = 0
    for qid, data in new_qids.items():
        if qid in seen_qids:
            continue
        qid_num = int(qid[1:]) if qid[0] in ("Q", "P") else 0
        record = {
            "qid": qid_num,
            "is_property": qid.startswith("P"),
            "labels": data.get("labels", {}),
            "description": data.get("description", ""),
            "category": data.get("category", "entity"),
            "cross_refs": data.get("cross_refs", {}),
            "sitelinks": data.get("sitelinks", 0),
        }
        out_fh.write(json.dumps(record, ensure_ascii=False) + "\n")
        seen_qids.add(qid)
        written += 1

    return written


def _fetch_category(
    cat_label: str,
    p31_qid: str,
    cap: int,
    out_fh: Any,
    seen_qids: set[str],
    checkpoint: dict[str, Any],
    checkpoint_path: Path,
) -> int:
    """Fetch top-N items for a single P31 category."""
    if cap <= 0:
        logger.info("Skipping %s (cap=0)", cat_label)
        return 0

    offset = checkpoint.get(f"cat:{cat_label}", 0)
    if offset >= cap:
        logger.info("Category %s already complete (checkpoint=%d, cap=%d)",
                     cat_label, offset, cap)
        return 0

    total = 0
    batch_size = min(BATCH_SIZE, cap)

    while offset < cap:
        limit = min(batch_size, cap - offset)
        logger.info("Fetching %s (%s) offset=%d limit=%d cap=%d",
                     cat_label, p31_qid, offset, limit, cap)

        try:
            query = _build_category_query(p31_qid, limit, offset)
            result = _sparql_request(query)
            bindings = result.get("results", {}).get("bindings", [])
        except Exception as exc:
            logger.error("Failed to fetch %s at offset %d: %s",
                         cat_label, offset, exc)
            break

        if not bindings:
            logger.info("No more results for %s at offset %d", cat_label, offset)
            break

        batch_data: dict[str, dict[str, Any]] = {}
        for row in bindings:
            item_uri = row.get("item", {}).get("value", "")
            qid = _parse_qid(item_uri)
            if qid is None or not qid.startswith("Q"):
                continue

            desc = row.get("description", {}).get("value", "")
            sitelinks = int(row.get("sitelinks", {}).get("value", "0"))
            cross_refs: dict[str, str] = {}
            for prop in CROSS_REF_PROPERTIES:
                val = row.get(prop, {}).get("value")
                if val:
                    cross_refs[prop] = val

            batch_data[qid] = {
                "description": desc,
                "category": cat_label,
                "cross_refs": cross_refs,
                "sitelinks": sitelinks,
            }

        written = _fetch_batch_with_labels(batch_data, out_fh, seen_qids)
        total += written
        offset += len(bindings)

        checkpoint[f"cat:{cat_label}"] = offset
        _save_checkpoint(checkpoint_path, checkpoint)

        if len(bindings) < limit:
            break

    logger.info("Category %s: %d new concepts", cat_label, total)
    return total


# ---------------------------------------------------------------------------
# Main entry point
# ---------------------------------------------------------------------------

def fetch_all(
    output_path: Path,
    checkpoint_dir: Path,
    quick: bool = False,
) -> int:
    """Fetch Wikidata concepts using popularity-based strategy.

    For each P31 category, fetches top-N items ordered by sitelinks count
    (= number of Wikipedia articles = popularity proxy). Then does a
    general pass for popular items that don't fit specific categories.

    Args:
        output_path: Path to the output JSONL file.
        checkpoint_dir: Directory for checkpoint files.
        quick: If True, fetch only 100K concepts total.

    Returns:
        Total number of concepts written.
    """
    checkpoint_path = checkpoint_dir / "wikidata_checkpoint.json"
    checkpoint = _load_checkpoint(checkpoint_path)

    output_path.parent.mkdir(parents=True, exist_ok=True)
    checkpoint_dir.mkdir(parents=True, exist_ok=True)

    # Track all seen QIDs to avoid duplicates across categories
    seen_qids: set[str] = set()

    # Load existing QIDs from output file if resuming
    if output_path.exists() and output_path.stat().st_size > 0:
        logger.info("Loading existing QIDs from %s for resume...", output_path)
        with open(output_path, "r", encoding="utf-8") as fh:
            for line in fh:
                try:
                    rec = json.loads(line)
                    prefix = "P" if rec.get("is_property") else "Q"
                    seen_qids.add(f"{prefix}{rec['qid']}")
                except (json.JSONDecodeError, KeyError):
                    pass
        logger.info("Loaded %d existing QIDs", len(seen_qids))

    total_written = len(seen_qids)

    # Open for append (resume-safe)
    with open(output_path, "a", encoding="utf-8") as out_fh:

        if quick:
            # Quick mode: general top 100K only
            logger.info("QUICK MODE: fetching top 100K by sitelinks")
            general_offset = checkpoint.get("general_top", 0)
            while total_written < 100_000 and general_offset < 200_000:
                limit = min(BATCH_SIZE, 100_000 - total_written)
                try:
                    query = _build_general_top_query(limit, general_offset)
                    result = _sparql_request(query)
                    bindings = result.get("results", {}).get("bindings", [])
                except Exception as exc:
                    logger.error("Quick fetch failed: %s", exc)
                    break
                if not bindings:
                    break
                batch_data: dict[str, dict[str, Any]] = {}
                for row in bindings:
                    qid = _parse_qid(row.get("item", {}).get("value", ""))
                    if qid is None or qid in seen_qids:
                        continue
                    batch_data[qid] = {
                        "description": row.get("description", {}).get("value", ""),
                        "category": "general",
                        "cross_refs": {},
                        "sitelinks": int(row.get("sitelinks", {}).get("value", "0")),
                    }
                written = _fetch_batch_with_labels(batch_data, out_fh, seen_qids)
                total_written += written
                general_offset += len(bindings)
                checkpoint["general_top"] = general_offset
                _save_checkpoint(checkpoint_path, checkpoint)
                out_fh.flush()
                if len(bindings) < limit:
                    break
        else:
            # Phase 1: Per-category fetch with caps
            categories = [(k, v[0], v[1]) for k, v in CATEGORY_CAPS.items()]
            pbar = tqdm(categories, desc="Wikidata categories", unit="cat")

            for cat_label, p31_qid, cap in pbar:
                pbar.set_postfix(category=cat_label, total=f"{total_written:,}")
                count = _fetch_category(
                    cat_label, p31_qid, cap,
                    out_fh, seen_qids,
                    checkpoint, checkpoint_path,
                )
                total_written += count
                out_fh.flush()

            # Phase 2: General top sitelinks pass
            general_cap = GENERAL_TOP_CAP
            general_offset = checkpoint.get("general_top", 0)
            logger.info("Phase 2: General top sitelinks (cap=%d, offset=%d)",
                        general_cap, general_offset)

            while general_offset < general_cap:
                limit = min(BATCH_SIZE, general_cap - general_offset)
                logger.info("General top: offset=%d limit=%d total=%d",
                            general_offset, limit, total_written)
                try:
                    query = _build_general_top_query(limit, general_offset)
                    result = _sparql_request(query)
                    bindings = result.get("results", {}).get("bindings", [])
                except Exception as exc:
                    logger.error("General top failed at offset %d: %s",
                                 general_offset, exc)
                    break
                if not bindings:
                    break
                batch_data2: dict[str, dict[str, Any]] = {}
                for row in bindings:
                    qid = _parse_qid(row.get("item", {}).get("value", ""))
                    if qid is None or qid in seen_qids:
                        continue
                    desc = row.get("description", {}).get("value", "")
                    sitelinks = int(row.get("sitelinks", {}).get("value", "0"))
                    cross_refs: dict[str, str] = {}
                    for prop in CROSS_REF_PROPERTIES:
                        val = row.get(prop, {}).get("value")
                        if val:
                            cross_refs[prop] = val
                    batch_data2[qid] = {
                        "description": desc, "category": "general",
                        "cross_refs": cross_refs, "sitelinks": sitelinks,
                    }
                written = _fetch_batch_with_labels(batch_data2, out_fh, seen_qids)
                total_written += written
                general_offset += len(bindings)
                checkpoint["general_top"] = general_offset
                _save_checkpoint(checkpoint_path, checkpoint)
                out_fh.flush()
                if len(bindings) < limit:
                    break

            # Phase 3: Properties (P-items)
            if FETCH_PROPERTIES:
                prop_offset = checkpoint.get("properties", 0)
                logger.info("Phase 3: Wikidata properties (P-items)")
                while prop_offset < PROPERTY_CAP:
                    limit = min(BATCH_SIZE, PROPERTY_CAP - prop_offset)
                    try:
                        query = _build_property_query(limit, prop_offset)
                        result = _sparql_request(query)
                        bindings = result.get("results", {}).get("bindings", [])
                    except Exception as exc:
                        logger.error("Property fetch failed: %s", exc)
                        break
                    if not bindings:
                        break
                    batch_data3: dict[str, dict[str, Any]] = {}
                    for row in bindings:
                        qid = _parse_qid(row.get("item", {}).get("value", ""))
                        if qid is None or qid in seen_qids:
                            continue
                        batch_data3[qid] = {
                            "description": row.get("description", {}).get("value", ""),
                            "category": "property", "cross_refs": {}, "sitelinks": 0,
                        }
                    written = _fetch_batch_with_labels(batch_data3, out_fh, seen_qids)
                    total_written += written
                    prop_offset += len(bindings)
                    checkpoint["properties"] = prop_offset
                    _save_checkpoint(checkpoint_path, checkpoint)
                    out_fh.flush()
                    if len(bindings) < limit:
                        break

    logger.info("Wikidata fetch complete: %d concepts written to %s",
                total_written, output_path)
    return total_written


if __name__ == "__main__":
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )
    from config import CHECKPOINT_DIR, RAW_DIR

    RAW_DIR.mkdir(parents=True, exist_ok=True)
    CHECKPOINT_DIR.mkdir(parents=True, exist_ok=True)
    fetch_all(RAW_DIR / "wikidata.jsonl", CHECKPOINT_DIR, quick=True)
