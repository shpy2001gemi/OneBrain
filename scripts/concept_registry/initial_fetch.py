"""
Initial fetch orchestrator for the OneBrain Concept Registry.

Runs each source fetcher sequentially, deduplicates, and builds
the final ``.obr`` binary. Provides CLI flags for quick mode and
source selection.
"""

import argparse
import logging
import sys
import time
from pathlib import Path
from typing import Any

from config import (
    CHECKPOINT_DIR,
    MERGED_DIR,
    OBR_OUTPUT,
    RAW_DIR,
)

logger = logging.getLogger(__name__)


def main() -> None:
    """Entry point for the initial concept registry fetch."""
    parser = argparse.ArgumentParser(
        description="OneBrain Concept Registry — Initial Fetch",
    )
    parser.add_argument(
        "--quick",
        action="store_true",
        help="Quick mode: fetch only top 100K Wikidata entities (by sitelinks).",
    )
    parser.add_argument(
        "--sources",
        type=str,
        default="wd,gn,ncbi,chebi,en",
        help="Comma-separated list of sources to fetch. "
             "Options: wd, gn, ncbi, chebi, en (default: all).",
    )
    parser.add_argument(
        "--output-dir",
        type=str,
        default=None,
        help="Override the OBR output directory.",
    )
    parser.add_argument(
        "--wd-top-n",
        type=int,
        default=10_000_000,
        help="Number of top Wikidata concepts to keep after quality ranking "
             "(default: 10,000,000).",
    )
    args = parser.parse_args()

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )

    # Ensure directories exist
    RAW_DIR.mkdir(parents=True, exist_ok=True)
    MERGED_DIR.mkdir(parents=True, exist_ok=True)
    CHECKPOINT_DIR.mkdir(parents=True, exist_ok=True)

    obr_output = Path(args.output_dir) / "concepts.obr" if args.output_dir else OBR_OUTPUT
    obr_output.parent.mkdir(parents=True, exist_ok=True)

    sources = [s.strip().lower() for s in args.sources.split(",")]
    timings: dict[str, float] = {}
    counts: dict[str, int] = {}

    overall_start = time.time()

    # ------------------------------------------------------------------
    # Source fetchers
    # ------------------------------------------------------------------

    if "wd" in sources:
        logger.info("=" * 60)
        logger.info("STAGE: Wikidata fetch (dump-based, target=10M concepts)")
        logger.info("=" * 60)
        t0 = time.time()
        from sources.wikidata_dump import fetch_all as wd_fetch

        count = wd_fetch(
            RAW_DIR / "wikidata.jsonl",
            CHECKPOINT_DIR,
            target_count=50_000_000,  # Collect ALL, rank later
        )
        elapsed = time.time() - t0
        timings["wikidata"] = elapsed
        counts["wikidata"] = count
        logger.info("Wikidata: %d concepts in %.1fs", count, elapsed)

    if "gn" in sources:
        logger.info("=" * 60)
        logger.info("STAGE: GeoNames fetch")
        logger.info("=" * 60)
        t0 = time.time()
        from sources.geonames import fetch_all as gn_fetch

        count = gn_fetch(RAW_DIR / "geonames.jsonl", CHECKPOINT_DIR)
        elapsed = time.time() - t0
        timings["geonames"] = elapsed
        counts["geonames"] = count
        logger.info("GeoNames: %d concepts in %.1fs", count, elapsed)

    if "ncbi" in sources:
        logger.info("=" * 60)
        logger.info("STAGE: NCBI Taxonomy fetch")
        logger.info("=" * 60)
        t0 = time.time()
        from sources.ncbi_taxonomy import fetch_all as ncbi_fetch

        count = ncbi_fetch(RAW_DIR / "ncbi_taxonomy.jsonl", CHECKPOINT_DIR)
        elapsed = time.time() - t0
        timings["ncbi"] = elapsed
        counts["ncbi"] = count
        logger.info("NCBI: %d concepts in %.1fs", count, elapsed)

    if "chebi" in sources:
        logger.info("=" * 60)
        logger.info("STAGE: ChEBI fetch")
        logger.info("=" * 60)
        t0 = time.time()
        from sources.chebi import fetch_all as chebi_fetch

        count = chebi_fetch(RAW_DIR / "chebi.jsonl", CHECKPOINT_DIR)
        elapsed = time.time() - t0
        timings["chebi"] = elapsed
        counts["chebi"] = count
        logger.info("ChEBI: %d concepts in %.1fs", count, elapsed)

    if "en" in sources:
        logger.info("=" * 60)
        logger.info("STAGE: English Dictionary (WordNet)")
        logger.info("=" * 60)
        t0 = time.time()
        from sources.english_dict import fetch_all as en_fetch

        count = en_fetch(RAW_DIR / "english_dict.jsonl", CHECKPOINT_DIR)
        elapsed = time.time() - t0
        timings["english_dict"] = elapsed
        counts["english_dict"] = count
        logger.info("English Dict: %d entries in %.1fs", count, elapsed)

    # ------------------------------------------------------------------
    # Wikidata ranking (if wikidata was fetched)
    # ------------------------------------------------------------------
    wd_raw = RAW_DIR / "wikidata.jsonl"
    wd_ranked = RAW_DIR / "wikidata_ranked.jsonl"
    if wd_raw.exists() and "wd" in sources:
        logger.info("=" * 60)
        logger.info("STAGE: Rank Wikidata (top %s by quality)", f"{args.wd_top_n:,}")
        logger.info("=" * 60)
        t0 = time.time()
        from rank_wikidata import rank_and_select

        rank_count = rank_and_select(wd_raw, wd_ranked, top_n=args.wd_top_n)
        elapsed = time.time() - t0
        timings["rank_wd"] = elapsed
        counts["rank_wd"] = rank_count
        logger.info("Ranked: %d -> %d in %.1fs",
                     counts.get("wikidata", 0), rank_count, elapsed)

    # ------------------------------------------------------------------
    # Deduplication
    # ------------------------------------------------------------------
    logger.info("=" * 60)
    logger.info("STAGE: Deduplication")
    logger.info("=" * 60)
    t0 = time.time()
    from dedup import deduplicate

    dedup_stats = deduplicate(RAW_DIR, MERGED_DIR / "concepts_deduped.jsonl")
    elapsed = time.time() - t0
    timings["dedup"] = elapsed
    logger.info("Dedup: %d -> %d in %.1fs",
                dedup_stats["total_input"],
                dedup_stats["final_count"],
                elapsed)

    # ------------------------------------------------------------------
    # Build OBR binary
    # ------------------------------------------------------------------
    logger.info("=" * 60)
    logger.info("STAGE: Build OBR")
    logger.info("=" * 60)
    t0 = time.time()
    from build_obr import build

    obr_stats = build(MERGED_DIR / "concepts_deduped.jsonl", obr_output)
    elapsed = time.time() - t0
    timings["build_obr"] = elapsed
    logger.info("OBR build: %d entries, %.2f MB in %.1fs",
                obr_stats["entries"],
                obr_stats["file_size_mb"],
                elapsed)

    # ------------------------------------------------------------------
    # Summary
    # ------------------------------------------------------------------
    total_elapsed = time.time() - overall_start

    logger.info("")
    logger.info("=" * 60)
    logger.info("PIPELINE COMPLETE")
    logger.info("=" * 60)
    logger.info("")

    for stage, t in timings.items():
        count_str = f" ({counts[stage]:,} concepts)" if stage in counts else ""
        logger.info("  %-15s %8.1fs%s", stage, t, count_str)

    logger.info("")
    logger.info("  %-15s %8.1fs", "TOTAL", total_elapsed)
    logger.info("")
    logger.info("  Output: %s (%.2f MB)", obr_output, obr_stats["file_size_mb"])
    logger.info("  Entries: %s", f"{obr_stats['entries']:,}")
    logger.info("  Labels:  %s", f"{obr_stats['labels']:,}")
    logger.info("  CCID collisions: %d", obr_stats["collisions"])


if __name__ == "__main__":
    main()
