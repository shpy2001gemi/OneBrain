# OneBrain Concept Registry — Build Guide

## Overview

The Concept Registry pipeline downloads, processes, and ranks concept data from
multiple sources, then builds a compact binary `.obr` file used by OneBrain's
AI engine.

**Final output:** `onebrain_data/concepts.obr`

### Data Sources

| Source | Description | Size (download) | Records |
|--------|-------------|-----------------|---------|
| **Wikidata** | General knowledge concepts from Wikidata dump | ~155 GB (.gz) | ~25M raw → ranked top N |
| **English Dictionary** | WordNet words, phrases, idioms with POS/synonyms | ~14 MB (NLTK) | ~147K |
| **GeoNames** | Geographic places worldwide | ~400 MB | ~3.3M |
| **NCBI Taxonomy** | Biological species taxonomy | ~60 MB | ~24.6M |
| **ChEBI** | Chemical compounds | ~20 MB | ~50K |

---

## Prerequisites

### Software
- **Python 3.11+**
- **curl** (for download script, optional)
- Required packages:
  ```bash
  pip install tqdm blake3 nltk
  ```

### NLTK Data (for English Dictionary)
```bash
python -c "import nltk; nltk.download('wordnet'); nltk.download('omw-1.4')"
```

### Disk Space
- `checkpoints/` — ~160 GB (Wikidata dump)
- `raw/` — ~20 GB (extracted JSONL files)
- `merged/` — ~10 GB (deduplicated JSONL)
- `onebrain_data/` — ~1.3 GB (final OBR binary)

---

## Step-by-Step Instructions

### Step 0: Download Wikidata Dump (Manual)

The Wikidata dump is ~155 GB. **Download manually via browser** for best speed.

1. Go to: **https://dumps.wikimedia.org/wikidatawiki/entities/**
2. Click the **latest dated folder** (e.g., `20260713/`)
3. Download: **`wikidata-YYYYMMDD-all.json.gz`**
   - Alternative: download from the root: `latest-all.json.gz`
   - ⚠️ Download the `.json.gz` file, **NOT** `.json.bz2`
4. Move the downloaded file to:
   ```
   scripts/concept_registry/checkpoints/
   ```

The pipeline auto-detects any of these filenames:
- `latest-all.json.gz`
- `wikidata-*-all.json.gz`
- `wikidata-*-all.json.bz2`

**Alternatively**, use the download script (slower, but auto-resumes):
```bash
cd scripts/concept_registry
python download_dump.py          # resume existing download
python download_dump.py --fresh  # delete old file and start over
```

---

### Step 1: Fetch & Rank Wikidata

```bash
cd scripts/concept_registry
python initial_fetch.py --sources wd
```

**What this does:**
1. Reads the `.gz` dump from `checkpoints/`
2. Extracts all valid concepts (MIN_SITELINKS=0, English labels required)
3. Writes `raw/wikidata.jsonl` (~25M records)
4. **Ranks** by quality score: `sitelinks × 10 + min(len(description), 500)`
5. Selects **top 10,000,000** → writes `raw/wikidata_ranked.jsonl`
6. Runs deduplication → builds OBR

**⏱ Duration:** ~4–5 hours (depends on CPU and disk speed)

#### Customize the rank count

To keep a different number of top concepts (e.g., 15 million):

```bash
python initial_fetch.py --sources wd --wd-top-n 15000000
```

| `--wd-top-n` | Description |
|--------------|-------------|
| `5000000` | Top 5M — highest quality only |
| `10000000` | Top 10M — default, recommended |
| `15000000` | Top 15M — broader coverage |
| `25000000` | All ~25M — everything that passes filters |

---

### Step 2: Fetch English Dictionary

```bash
python initial_fetch.py --sources en
```

**What this does:**
1. Downloads WordNet data via NLTK (if not cached)
2. Extracts ~147K words and phrases with definitions, POS, synonyms, hypernyms
3. Writes `raw/english_dict.jsonl`
4. **Merge Strategy B:** enriches matching Wikidata records with POS/synonyms,
   adds non-matching words as new entries
5. Runs deduplication → rebuilds OBR

**⏱ Duration:** ~7 minutes

> **Note:** This step also re-runs dedup and OBR build, incorporating all
> existing raw files (Wikidata, GeoNames, etc.).

---

### Step 3 (Optional): Fetch Other Sources

If this is the first run, fetch the remaining scientific sources:

```bash
python initial_fetch.py --sources gn,ncbi,chebi
```

**What this does:**
1. Downloads GeoNames allCountries.zip (~400 MB)
2. Downloads NCBI taxdump.tar.gz (~60 MB)
3. Downloads ChEBI compounds (~20 MB)
4. Processes each into JSONL
5. Runs deduplication → rebuilds OBR

**⏱ Duration:** ~30–60 minutes

> **Note:** If raw JSONL files already exist from a previous run, the dedup
> step will automatically include them even if you don't re-fetch.

---

### Step 4: Full Pipeline (All Sources at Once)

To run everything in a single command:

```bash
python initial_fetch.py
```

Or with a custom Wikidata rank:

```bash
python initial_fetch.py --wd-top-n 15000000
```

This is equivalent to running Steps 1 + 2 + 3 sequentially.

---

## Pipeline Architecture

```
checkpoints/
  └── wikidata-YYYYMMDD-all.json.gz     ← Manual download (155 GB)

          │
          ▼
┌─────────────────────────────────┐
│  initial_fetch.py --sources wd  │
│                                 │
│  1. Decompress + parse gz       │
│  2. Filter by P31 + EN label    │
│  3. Write raw/wikidata.jsonl    │
│  4. Rank → wikidata_ranked.jsonl│
└─────────────────────────────────┘
          │
          ▼
┌─────────────────────────────────┐
│  initial_fetch.py --sources en  │
│                                 │
│  1. NLTK WordNet → JSONL        │
│  2. Write raw/english_dict.jsonl│
└─────────────────────────────────┘
          │
          ▼
┌─────────────────────────────────┐
│  Deduplication (dedup.py)       │
│                                 │
│  - Cross-ref matching (WD↔GN…) │
│  - EN Dict merge (Strategy B)  │
│  → merged/concepts_deduped.jsonl│
└─────────────────────────────────┘
          │
          ▼
┌─────────────────────────────────┐
│  Build OBR (build_obr.py)       │
│                                 │
│  - Blake3 CCID hashing          │
│  - Binary packing               │
│  → onebrain_data/concepts.obr  │
└─────────────────────────────────┘
```

---

## File Structure

```
scripts/concept_registry/
├── initial_fetch.py          # Main orchestrator
├── download_dump.py          # Wikidata download helper
├── rank_wikidata.py          # Quality ranking (standalone)
├── dedup.py                  # Cross-source deduplication
├── build_obr.py              # OBR binary builder
├── config.py                 # Shared constants
├── sources/
│   ├── wikidata_dump.py      # Wikidata gz parser
│   ├── english_dict.py       # WordNet dictionary
│   ├── geonames.py           # GeoNames
│   ├── ncbi_taxonomy.py      # NCBI Taxonomy
│   └── chebi.py              # ChEBI chemicals
├── checkpoints/              # Downloaded dumps
│   └── wikidata-*.json.gz
├── raw/                      # Extracted JSONL per source
│   ├── wikidata.jsonl
│   ├── wikidata_ranked.jsonl
│   ├── english_dict.jsonl
│   ├── geonames.jsonl
│   ├── ncbi_taxonomy.jsonl
│   └── chebi.jsonl
├── merged/
│   └── concepts_deduped.jsonl
└── ../../onebrain_data/
    └── concepts.obr           # Final output
```

---

## Quality Ranking Details

The Wikidata ranking uses a composite quality score:

```
quality_score = sitelinks × 10 + min(len(description), 500)
```

- **sitelinks** = number of Wikipedia language editions linking to this concept
  (higher = more globally recognized)
- **description length** = secondary tiebreaker for concepts with equal sitelinks

Example distribution (from a 25M collection, top 10M selected):

| Sitelinks Range | Estimated Count | Examples |
|----------------|-----------------|---------|
| ≥ 100 | ~50K | "computer", "democracy", "water" |
| 10–99 | ~500K | "semaphore", "feudalism", "acetone" |
| 2–9 | ~3M | Notable but less universal concepts |
| 1 | ~1M | Has at least one Wikipedia article |
| 0 | ~5.5M | Wikidata-only (filled by description quality) |

---

## English Dictionary Merge (Strategy B)

For each WordNet word/phrase:

1. **Match by English label** against Wikidata concepts (case-insensitive)
2. **If match found** → enrich the Wikidata record:
   - Add `pos` (part of speech: noun, verb, adj, adv)
   - Add `synonyms`
   - Add `hypernyms`
3. **If no match** → create new concept entry from dictionary

This ensures Wikidata concepts gain linguistic metadata without duplication.

---

## Troubleshooting

### "Resource 'wordnet' not found"
```bash
python -c "import nltk; nltk.download('wordnet'); nltk.download('omw-1.4')"
```

### "struct.error: required argument is not an integer"
English dictionary entries use string IDs — ensure `build_obr.py` has the
string-to-hash conversion (already fixed in current version).

### Pipeline stuck at 25% / same concept count
Delete old output to prevent stale QID cache:
```bash
del raw\wikidata.jsonl
del checkpoints\wikidata_dump_checkpoint.json
```

### Gzip corruption mid-stream
The pipeline handles gzip corruption gracefully — it saves all data collected
before the corruption point and continues to dedup/OBR build. If the dump file
is incomplete, re-download it.

### Slow download via `download_dump.py`
Download manually via browser (see Step 0). Browser download managers and IDM
are significantly faster for large files.

---

## CLI Reference

```
usage: initial_fetch.py [-h] [--quick] [--sources SOURCES]
                        [--output-dir OUTPUT_DIR] [--wd-top-n WD_TOP_N]

Options:
  --sources SOURCES     Comma-separated source list: wd,gn,ncbi,chebi,en
  --wd-top-n WD_TOP_N  Top N Wikidata concepts to keep (default: 10,000,000)
  --quick               Quick mode: 100K entities only (for testing)
  --output-dir DIR      Override OBR output path
```

### Examples

```bash
# Full pipeline, all sources, default 10M Wikidata
python initial_fetch.py

# Wikidata only, keep top 15M
python initial_fetch.py --sources wd --wd-top-n 15000000

# English dictionary only (re-runs dedup + OBR build)
python initial_fetch.py --sources en

# Quick test with 100K concepts
python initial_fetch.py --sources wd --quick

# Standalone ranking (if wikidata.jsonl already exists)
python rank_wikidata.py
```
