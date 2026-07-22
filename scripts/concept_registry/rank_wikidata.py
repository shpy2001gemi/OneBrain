#!/usr/bin/env python3
"""rank_wikidata.py – Rank Wikidata JSONL by quality score and keep top 10M.

Two-pass approach for memory efficiency on ~36M records:
  Pass 1 (binary): scan every line, compute quality_score, store
         (quality_score, byte_offset, line_length) in a list.
  Sort descending by quality_score.
  Pass 2 (binary seek): read only the top-N lines back from the
         original file and write them out as ranked JSONL.
"""

from __future__ import annotations

import json
import logging
import struct
import sys
import time
from pathlib import Path

from tqdm import tqdm

# ── paths ────────────────────────────────────────────────────────────────
SCRIPT_DIR = Path(__file__).parent
INPUT_PATH = SCRIPT_DIR / "raw" / "wikidata.jsonl"
OUTPUT_PATH = SCRIPT_DIR / "raw" / "wikidata_ranked.jsonl"
TOP_N = 10_000_000

# ── logging ──────────────────────────────────────────────────────────────
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s  %(levelname)-8s  %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
log = logging.getLogger(__name__)


def compute_quality_score(record: dict) -> int:
    """quality_score = sitelinks * 10 + min(len(description), 500)"""
    sitelinks = record.get("sitelinks", 0) or 0
    description = record.get("description", "") or ""
    return sitelinks * 10 + min(len(description), 500)


def pass1_scan(input_path: Path) -> list[tuple[int, int, int]]:
    """Pass 1 – read every line in binary mode, return list of
    (quality_score, byte_offset, line_length) tuples.
    """
    file_size = input_path.stat().st_size
    entries: list[tuple[int, int, int]] = []

    log.info("Pass 1: scanning %s (%.2f GB) …", input_path.name, file_size / 1e9)

    with open(input_path, "rb") as fh:
        pbar = tqdm(
            total=file_size,
            unit="B",
            unit_scale=True,
            desc="Pass 1 – scan",
            dynamic_ncols=True,
        )
        while True:
            offset = fh.tell()
            raw_line = fh.readline()
            if not raw_line:
                break

            line_len = len(raw_line)
            pbar.update(line_len)

            # skip blank lines
            stripped = raw_line.strip()
            if not stripped:
                continue

            try:
                record = json.loads(stripped)
            except json.JSONDecodeError:
                continue

            score = compute_quality_score(record)
            entries.append((score, offset, line_len))

        pbar.close()

    log.info("Pass 1 done – %s records scanned.", f"{len(entries):,}")
    return entries


def pass2_write(
    input_path: Path,
    output_path: Path,
    entries: list[tuple[int, int, int]],
    top_n: int,
) -> dict[str, int]:
    """Pass 2 – sort entries, take top_n, seek-read each line, write out.
    Returns sitelinks distribution counts.
    """
    log.info("Sorting %s entries by quality_score descending …", f"{len(entries):,}")
    t0 = time.perf_counter()
    entries.sort(key=lambda e: e[0], reverse=True)
    log.info("Sort completed in %.1f s.", time.perf_counter() - t0)

    selected = entries[:top_n]
    log.info(
        "Selected top %s records (min score=%s, max score=%s).",
        f"{len(selected):,}",
        selected[-1][0] if selected else "n/a",
        selected[0][0] if selected else "n/a",
    )

    # distribution buckets
    dist = {
        "sitelinks_gte_100": 0,
        "sitelinks_10_99": 0,
        "sitelinks_2_9": 0,
        "sitelinks_1": 0,
        "sitelinks_0": 0,
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)

    with (
        open(input_path, "rb") as fin,
        open(output_path, "w", encoding="utf-8") as fout,
    ):
        for score, offset, line_len in tqdm(
            selected, desc="Pass 2 – write", dynamic_ncols=True
        ):
            fin.seek(offset)
            raw_line = fin.read(line_len)
            record = json.loads(raw_line)

            # distribution stats
            sl = record.get("sitelinks", 0) or 0
            if sl >= 100:
                dist["sitelinks_gte_100"] += 1
            elif sl >= 10:
                dist["sitelinks_10_99"] += 1
            elif sl >= 2:
                dist["sitelinks_2_9"] += 1
            elif sl == 1:
                dist["sitelinks_1"] += 1
            else:
                dist["sitelinks_0"] += 1

            fout.write(json.dumps(record, ensure_ascii=False) + "\n")

    return dist


def rank_and_select(
    input_path: Path,
    output_path: Path,
    top_n: int = TOP_N,
) -> int:
    """Rank Wikidata records by quality and write top N.

    Args:
        input_path: Path to raw wikidata.jsonl
        output_path: Path for ranked output
        top_n: Number of top records to keep

    Returns:
        Number of records written.
    """
    if not input_path.exists():
        log.error("Input file not found: %s", input_path)
        return 0

    t_start = time.perf_counter()

    # Pass 1 - scan
    entries = pass1_scan(input_path)

    if not entries:
        log.warning("No valid records found. Nothing to write.")
        return 0

    # Pass 2 - sort, select, write
    dist = pass2_write(input_path, output_path, entries, top_n)

    actual_written = sum(dist.values())
    elapsed = time.perf_counter() - t_start

    # Distribution report
    log.info("--- Distribution stats (top %s) ---", f"{actual_written:,}")
    log.info("  sitelinks >= 100 : %s", f"{dist['sitelinks_gte_100']:>10,}")
    log.info("  sitelinks 10-99  : %s", f"{dist['sitelinks_10_99']:>10,}")
    log.info("  sitelinks  2-9   : %s", f"{dist['sitelinks_2_9']:>10,}")
    log.info("  sitelinks  1     : %s", f"{dist['sitelinks_1']:>10,}")
    log.info("  sitelinks  0     : %s", f"{dist['sitelinks_0']:>10,}")
    log.info("Output written to %s", output_path)
    log.info("Total elapsed: %.1f s", elapsed)

    return actual_written


def main() -> None:
    rank_and_select(INPUT_PATH, OUTPUT_PATH, TOP_N)


if __name__ == "__main__":
    main()

