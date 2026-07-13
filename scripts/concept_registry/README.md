# OneBrain Concept Registry — Data Pipeline

Fetches, deduplicates, and compiles ~8M concepts from four authoritative
sources into a compact binary `.obr` file for the **KU v7 ConceptRegistry**.

## Prerequisites

- **Python 3.10+**
- Internet access for initial downloads (~500 MB total)

## Installation

```bash
cd scripts/concept_registry
pip install -r requirements.txt
```

## Usage

### Full initial fetch (all ~8M concepts)

```bash
python initial_fetch.py
```

Expect this to take **several hours** due to Wikidata rate limits.

### Quick test run (~100K concepts)

```bash
python initial_fetch.py --quick
```

### Fetch specific sources only

```bash
python initial_fetch.py --sources wd,gn
```

### Quarterly incremental update

```bash
python quarterly_update.py
```

### Dry-run (see what would change)

```bash
python quarterly_update.py --dry-run
```

## Data Sources

| Source | URL | Records | Content |
|--------|-----|---------|---------|
| **Wikidata** | `query.wikidata.org/sparql` | ~5M | Entities across 18 P31 types |
| **GeoNames** | `download.geonames.org` | ~2M | Geographic features & places |
| **NCBI Taxonomy** | `ftp.ncbi.nlm.nih.gov` | ~500K | Species & genus taxa |
| **ChEBI** | `ftp.ebi.ac.uk` | ~150K | Chemical entities |

## Output Format

The pipeline produces a single binary `.obr` file at
`../../onebrain_data/concepts.obr` relative to this directory.

### OBR Binary Layout (little-endian)

```
Header (32 bytes):
  magic:       4B  "OBR1"
  version:     u32
  entry_count: u64
  label_count: u64
  reserved:    8B  (zeros)

Per entry:
  ccid:        16B (blake3 hash truncated to 128 bits)
  ext_id:      u32
  source:      u8  (0=Wikidata, 1=GeoNames, 2=NCBI, 3=ChEBI)
  category:    u8  (0=Entity, 1=Property, 2=Unit, 3=Taxon, 4=Place, ...)
  name_len:    u16
  name_bytes:  [u8; name_len]
  num_labels:  u16
  labels:      [label_len: u16, label_bytes: [u8; label_len]] × num_labels
```

### CCID Computation

Each concept gets a deterministic 128-bit ID using blake3:

```
blake3("wd:Q42")[:16]      # Wikidata entity Q42
blake3("gn:2643743")[:16]  # GeoNames London
blake3("ncbi:9606")[:16]   # NCBI Homo sapiens
blake3("chebi:15377")[:16] # ChEBI water
```

## File Structure

```
scripts/concept_registry/
├── README.md              # This file
├── requirements.txt       # Python dependencies
├── config.py              # Configuration constants
├── initial_fetch.py       # Full pipeline orchestrator
├── quarterly_update.py    # Incremental update script
├── dedup.py               # Cross-source deduplication
├── build_obr.py           # Binary .obr builder
├── sources/
│   ├── __init__.py
│   ├── wikidata.py        # Wikidata SPARQL fetcher
│   ├── geonames.py        # GeoNames dump parser
│   ├── ncbi_taxonomy.py   # NCBI Taxonomy parser
│   └── chebi.py           # ChEBI dump parser
├── raw/                   # (generated) Raw JSONL per source
├── merged/                # (generated) Deduplicated JSONL
└── checkpoints/           # (generated) Resumable state
```

## Category Codes

| Code | Name | Examples |
|------|------|----------|
| 0 | Entity | Organizations, diseases, genes, software |
| 1 | Property | Occupations |
| 2 | Unit | Units of measurement |
| 3 | Taxon | Species, genera |
| 4 | Place | Cities, countries, regions |
| 5 | Person | Humans |
| 6 | Event | Occurrences |
| 7 | Substance | Chemical compounds, proteins |
| 255 | Other | Uncategorised |

## Cross-Source Deduplication

Wikidata cross-reference properties are used to link entities across sources:

- `P683` → ChEBI ID
- `P846` → NCBI Taxonomy ID
- `P1566` → GeoNames ID
- `P231` → CAS Registry Number

When duplicates are found, the Wikidata record wins and labels from the
secondary source are merged in.
