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
import gzip
import hashlib
import json
import logging
import re
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from multiprocessing import cpu_count
from pathlib import Path
from typing import Optional

import requests
from tqdm import tqdm

try:
    import orjson
    _USE_ORJSON = True
except ImportError:
    _USE_ORJSON = False

logger = logging.getLogger(__name__)

# Regex to extract entity ID without full JSON parse
_FAST_ID_RE = re.compile(rb'"id"\s*:\s*"Q(\d+)"')
# Pre-build byte patterns for fast P31 exclusion check
_EXCLUDE_P31_BYTE_PATTERNS: list[bytes] = []

# Dated dump URL (updated weekly)
DUMP_URL = "https://dumps.wikimedia.org/wikidatawiki/entities/20260706/wikidata-20260706-all.json.gz"

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


def _init_exclude_patterns():
    """Build byte patterns for fast P31 string matching."""
    global _EXCLUDE_P31_BYTE_PATTERNS
    if not _EXCLUDE_P31_BYTE_PATTERNS:
        _EXCLUDE_P31_BYTE_PATTERNS = [
            f'"numeric-id":{qid}'.encode() for qid in sorted(EXCLUDE_P31)
        ]


def _fast_json_loads(data):
    """Use orjson if available, else stdlib json."""
    if _USE_ORJSON:
        return orjson.loads(data)
    if isinstance(data, bytes):
        return json.loads(data.decode("utf-8", errors="replace"))
    return json.loads(data)


def _fast_json_dumps(obj):
    """Use orjson if available, else stdlib json."""
    if _USE_ORJSON:
        return orjson.dumps(obj, option=orjson.OPT_APPEND_NEWLINE).decode("utf-8")
    return json.dumps(obj, ensure_ascii=False) + "\n"


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


class RobustBZ2LineReader:
    """Read lines from a multi-stream bz2 file, skipping corrupted blocks.

    Uses bz2.open() for fast reading (~1600 lines/s). On corruption,
    scans the raw file for the next 'BZh' stream header and resumes
    from there. Only entities within the corrupted block are lost.
    """

    _BZ2_MAGIC = b"BZh"

    def __init__(self, filepath: Path):
        self._filepath = filepath
        self._file_size = filepath.stat().st_size
        self._raw_pos = 0
        self._skipped_blocks = 0

    def __iter__(self):
        return self._iter_lines()

    def _iter_lines(self):
        """Yield decompressed lines with corruption recovery."""
        offset = 0

        while offset < self._file_size:
            try:
                # Open bz2 at current offset via a raw file handle
                fh = open(self._filepath, "rb")
                fh.seek(offset)
                bz2_fh = bz2.BZ2File(fh, "rb")

                for line in bz2_fh:
                    self._raw_pos = fh.tell()  # track compressed position
                    yield line

                # Clean EOF — done
                bz2_fh.close()
                fh.close()
                return

            except OSError as e:
                self._skipped_blocks += 1
                err_pos = fh.tell()
                err_pos_gb = err_pos / 1e9
                logger.warning(
                    "BZ2 corruption at ~%.2f GB (byte %d): %s (skip #%d)",
                    err_pos_gb, err_pos, e, self._skipped_blocks,
                )

                try:
                    bz2_fh.close()
                    fh.close()
                except Exception:
                    pass

                # Scan raw file from error position for next BZh header
                next_offset = self._find_next_header(err_pos + 1)
                if next_offset is not None:
                    skipped_mb = (next_offset - err_pos) / 1e6
                    logger.info(
                        "Recovery: found next BZ2 header at byte %d "
                        "(skipped %.1f MB). Resuming.",
                        next_offset, skipped_mb,
                    )
                    offset = next_offset
                else:
                    logger.error("No more BZ2 headers found. Stopping at %.2f GB.",
                                 err_pos / 1e9)
                    return

    def _find_next_header(self, start_pos: int) -> int | None:
        """Scan raw file from start_pos for next valid BZh header."""
        SCAN_CHUNK = 16 * 1024 * 1024  # 16 MB
        pos = start_pos

        with open(self._filepath, "rb") as fh:
            fh.seek(pos)
            while True:
                chunk = fh.read(SCAN_CHUNK)
                if not chunk:
                    return None

                idx = 0
                while True:
                    found = chunk.find(self._BZ2_MAGIC, idx)
                    if found < 0:
                        break
                    # Verify: BZh must be followed by block size digit 1-9
                    if found + 3 < len(chunk) and chr(chunk[found + 3]) in "123456789":
                        return pos + found
                    idx = found + 1

                pos += len(chunk)

    @property
    def raw_position(self) -> int:
        return self._raw_pos

    @property
    def skipped_blocks(self) -> int:
        return self._skipped_blocks

    def close(self):
        pass  # File handles are managed per-segment


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


def _parse_batch_worker(batch: list[bytes]) -> list[dict]:
    """Worker: parse a batch of raw JSON line bytes, return valid records.

    Runs in a separate process. Each line is a complete Wikidata entity JSON.
    Applies ALL filters: P31, sitelinks, labels — identical to _process_entity().
    Does NOT check seen_qids (main thread handles dedup).
    """
    results = []
    for raw_line_bytes in batch:
        raw_line_bytes = raw_line_bytes.rstrip(b",\n\r ")
        try:
            entity = _fast_json_loads(raw_line_bytes)
        except Exception:
            continue

        entity_id = entity.get("id", "")
        if not entity_id.startswith("Q"):
            continue

        qid = int(entity_id[1:])

        claims = entity.get("claims", {})
        p31_qids = _extract_p31_qids(claims)

        if not _is_concept(p31_qids):
            continue

        sitelinks = len(entity.get("sitelinks", {}))
        if sitelinks < MIN_SITELINKS:
            continue

        labels = _extract_labels(entity)
        if not labels.get("en"):
            continue

        descriptions = entity.get("descriptions", {})
        description = descriptions.get("en", {}).get("value", "")
        cross_refs = _extract_cross_refs(claims)

        results.append({
            "qid": qid,
            "labels": labels,
            "description": description,
            "category": "concept",
            "cross_refs": cross_refs,
            "sitelinks": sitelinks,
        })

    return results


def _find_decompressor(dump_path: Path) -> tuple[list[str], bool] | tuple[None, bool]:
    """Find bzip2 decompressor. Returns (cmd_list, is_parallel).

    NOTE: WSL lbzip2 is NOT used because it fails on Wikidata dumps
    with 'bad block header magic' after ~16GB of decompressed output.
    Native bzip2 (single-threaded) handles the full file correctly.
    ThreadPoolExecutor provides parallel JSON parsing on top.
    """
    # --- Native bzip2 (single-threaded, reliable) ---
    bz2_path = shutil.which("bzip2")
    if bz2_path:
        return [bz2_path, "-dc", str(dump_path)], False
    for p in [
        r"C:\Program Files\Git\usr\bin\bzip2.exe",
        r"C:\Program Files (x86)\Git\usr\bin\bzip2.exe",
    ]:
        if Path(p).exists():
            return [p, "-dc", str(dump_path)], False

    return None, False


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

    # Auto-detect local dump if not provided
    if dump_path is None:
        # Look for any .bz2 or .gz dump in checkpoint_dir
        for pattern in ["wikidata-*-all.json.bz2", "wikidata-*-all.json.gz"]:
            candidates = sorted(checkpoint_dir.glob(pattern), reverse=True)
            if candidates:
                dump_path = candidates[0]
                break

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

    # Determine source: local file or HTTP stream
    use_local = dump_path is not None and dump_path.exists()
    if use_local:
        source_size = dump_path.stat().st_size
        logger.info("Using LOCAL dump: %s (%.1f GB)", dump_path, source_size / 1e9)
    else:
        logger.info("No local dump found — streaming from %s", DUMP_URL)
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

    # Load byte position from checkpoint for local file resume
    resume_byte_pos = 0
    if use_local and checkpoint_path.exists():
        try:
            with open(checkpoint_path, "r") as f:
                cp_data = json.load(f)
            resume_byte_pos = cp_data.get("local_byte_pos", 0)
        except Exception:
            pass

    # Find best decompressor (WSL lbzip2 parallel > Git bzip2 single)
    if use_local:
        logger.info("Will use Python bz2 module (multi-stream safe)")

    local_fh = None  # Track file handle for cleanup

    while items_written < target_count and retry_count <= MAX_RETRIES:
        decoder = BZ2StreamDecoder()
        total_bytes_this_attempt = 0
        file_byte_pos = 0

        try:
            if use_local:
                # === LOCAL: detect format from extension ===
                ext = dump_path.suffix.lower()
                if ext == ".gz":
                    logger.info("Opening local dump with gzip (fast)...")
                    local_fh = gzip.open(dump_path, "rb")
                elif ext == ".bz2":
                    logger.info("Opening local dump with RobustBZ2LineReader (corruption-safe)...")
                    local_fh = RobustBZ2LineReader(dump_path)
                else:
                    raise ValueError(f"Unsupported dump format: {ext}")
                use_line_mode = True
            else:
                attempt_label = f" (attempt {retry_count + 1})" if retry_count > 0 else ""
                logger.info("Streaming from %s%s — have %d concepts, skipping seen QIDs",
                            DUMP_URL, attempt_label, items_written)
                resp, _ = _stream_from_http(DUMP_URL)
                use_line_mode = False

            # Log worker info BEFORE creating tqdm bars (prevents display corruption)
            num_workers = max(1, min(cpu_count() - 2, 16))
            BATCH_SIZE = 200
            if use_line_mode:
                logger.info("Using %d parallel workers for JSON parsing", num_workers)

            pbar_bytes = tqdm(total=source_size, unit="B", unit_scale=True,
                              desc="Decompress",
                              initial=file_byte_pos)
            pbar_items = tqdm(desc="Concepts", unit=" items",
                              initial=items_written, total=target_count)

            with open(output_path, "a", encoding="utf-8") as out_fh:
              if use_line_mode:
                # === PARALLEL: regex pre-filter + multiprocessing parse ===

                bytes_since_update = 0
                skipped_seen = 0
                candidate_batch: list[bytes] = []
                pending_futures = []

                pool = ThreadPoolExecutor(max_workers=num_workers)

                def _drain_completed():
                    """Non-blocking: collect results from done futures."""
                    nonlocal items_written
                    still_pending = []
                    for fut in pending_futures:
                        if fut.done():
                            try:
                                records = fut.result()
                            except Exception as e:
                                logger.warning("Worker batch error: %s", e)
                                continue
                            for record in records:
                                if record["qid"] in seen_qids:
                                    continue
                                out_fh.write(_fast_json_dumps(record))
                                seen_qids.add(record["qid"])
                                items_written += 1
                                stats["kept"] = items_written
                                pbar_items.update(1)
                        else:
                            still_pending.append(fut)
                    pending_futures.clear()
                    pending_futures.extend(still_pending)

                try:
                  last_checkpoint_time = time.time()
                  CHECKPOINT_SECS = 300  # 5 minutes

                  for raw_line_bytes in local_fh:
                    line_len = len(raw_line_bytes)
                    bytes_since_update += line_len

                    if bytes_since_update > 10 * 1024 * 1024:
                        # Use actual compressed position from reader (bz2 only)
                        if hasattr(local_fh, 'raw_position'):
                            new_pos = local_fh.raw_position
                        else:
                            # gzip: estimate from decompressed bytes
                            new_pos = file_byte_pos + bytes_since_update // 3
                        delta = new_pos - file_byte_pos
                        if delta > 0:
                            pbar_bytes.update(delta)
                            file_byte_pos = new_pos
                        total_bytes_this_attempt = new_pos
                        bytes_since_update = 0

                    # --- PRE-FILTER 1: skip trivially short/bracket lines ---
                    if line_len < 50:
                        continue

                    # --- PRE-FILTER 2: extract QID with regex (NO json parse) ---
                    id_match = _FAST_ID_RE.search(raw_line_bytes[:200])
                    if not id_match:
                        stats["non_q"] += 1
                        continue

                    qid = int(id_match.group(1))
                    if qid in seen_qids:
                        skipped_seen += 1
                        continue

                    # Add to batch for parallel processing
                    candidate_batch.append(raw_line_bytes)

                    if len(candidate_batch) >= BATCH_SIZE:
                        fut = pool.submit(_parse_batch_worker, candidate_batch)
                        pending_futures.append(fut)
                        candidate_batch = []

                        # Drain completed futures every batch submit
                        _drain_completed()

                        elapsed = time.time() - t0
                        pbar_items.set_postfix(
                            rate=f"{items_written / max(1, elapsed):.0f}/s",
                            w=num_workers, q=len(pending_futures),
                        )

                    # Time-based checkpoint (every 5 min)
                    now = time.time()
                    if now - last_checkpoint_time > CHECKPOINT_SECS:
                        _drain_completed()
                        out_fh.flush()
                        with open(checkpoint_path, "w") as cp:
                            json.dump({
                                "items_written": items_written,
                                "bytes_consumed": total_bytes_this_attempt,
                                "local_byte_pos": file_byte_pos,
                                "retries": retry_count,
                            }, cp)
                        elapsed = now - t0
                        logger.info(
                            "Checkpoint: %d concepts / ~%.1f GB (%.0f min, "
                            "skipped_seen=%dk, workers=%d)",
                            items_written, file_byte_pos / 1e9, elapsed / 60,
                            skipped_seen // 1000, num_workers,
                        )
                        last_checkpoint_time = now

                    if items_written >= target_count:
                        break

                  # Flush remaining batch
                  if candidate_batch:
                      fut = pool.submit(_parse_batch_worker, candidate_batch)
                      pending_futures.append(fut)

                  # Drain all remaining futures (blocking)
                  for fut in as_completed(pending_futures):
                      try:
                          records = fut.result()
                      except Exception as e:
                          logger.warning("Worker batch error (final): %s", e)
                          continue
                      for record in records:
                          if record["qid"] in seen_qids:
                              continue
                          out_fh.write(_fast_json_dumps(record))
                          seen_qids.add(record["qid"])
                          items_written += 1
                          stats["kept"] = items_written
                          pbar_items.update(1)
                          if items_written >= target_count:
                              break
                      if items_written >= target_count:
                          break
                  pending_futures.clear()

                finally:
                    pool.shutdown(wait=False, cancel_futures=True)

                if use_local and local_fh:
                    local_fh.close()

              else:
                # === CHUNK MODE: HTTP stream only ===
                data_iter = resp.iter_content(chunk_size=1024 * 1024)

                for chunk in data_iter:
                    total_bytes_this_attempt += len(chunk)
                    file_byte_pos += len(chunk)
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
                                    "local_byte_pos": file_byte_pos,
                                    "retries": retry_count,
                                }, cp)
                            logger.info(
                                "Checkpoint: %d concepts / %.1f GB parsed (%.0f min)",
                                items_written, file_byte_pos / 1e9, elapsed / 60,
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
                if local_fh:
                    local_fh.close()
                    local_fh = None
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
