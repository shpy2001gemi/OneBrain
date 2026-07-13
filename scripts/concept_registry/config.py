"""
Configuration constants for the OneBrain Concept Registry pipeline.

Defines source URLs, output paths, category mappings, rate limits,
and binary format constants used across all pipeline stages.
"""

from pathlib import Path
from typing import Optional

# ---------------------------------------------------------------------------
# Directory layout (relative to this script's directory)
# ---------------------------------------------------------------------------
SCRIPT_DIR = Path(__file__).resolve().parent
RAW_DIR = SCRIPT_DIR / "raw"
MERGED_DIR = SCRIPT_DIR / "merged"
CHECKPOINT_DIR = SCRIPT_DIR / "checkpoints"
OBR_OUTPUT = SCRIPT_DIR / ".." / ".." / "onebrain_data" / "concepts.obr"

# ---------------------------------------------------------------------------
# Source URLs
# ---------------------------------------------------------------------------
WIKIDATA_SPARQL_URL = "https://query.wikidata.org/sparql"
GEONAMES_DUMP_URL = "https://download.geonames.org/export/dump/allCountries.zip"
NCBI_TAXDUMP_URL = "https://ftp.ncbi.nlm.nih.gov/pub/taxonomy/taxdump.tar.gz"
CHEBI_COMPOUNDS_URL = "https://ftp.ebi.ac.uk/pub/databases/chebi/generic_dumps/generic_dump_allstar/compounds.sql.zip"
CHEBI_NAMES_URL = "https://ftp.ebi.ac.uk/pub/databases/chebi/generic_dumps/generic_dump_allstar/names.sql.zip"

# ---------------------------------------------------------------------------
# Wikidata P31 (instance-of) categories to fetch
# ---------------------------------------------------------------------------
WIKIDATA_P31_CATEGORIES: dict[str, str] = {
    "human": "Q5",
    "city": "Q515",
    "country": "Q6256",
    "region": "Q82794",
    "taxon": "Q16521",
    "chemical_compound": "Q11173",
    "occurrence": "Q1190554",
    "unit": "Q47574",
    "organization": "Q43229",
    "disease": "Q12136",
    "protein": "Q8054",
    "gene": "Q7187",
    "software": "Q7397",
    "language": "Q34",
    "film": "Q11424",
    "book": "Q571",
    "sport": "Q349",
    "occupation": "Q12737",
}

# ---------------------------------------------------------------------------
# Label languages to retrieve from Wikidata
# ---------------------------------------------------------------------------
LABEL_LANGUAGES: list[str] = ["en", "vi", "fr", "de", "es", "ja", "zh", "ko"]

# ---------------------------------------------------------------------------
# Rate limits
# ---------------------------------------------------------------------------
WIKIDATA_DELAY: float = 2.0  # seconds between SPARQL requests
BATCH_SIZE: int = 10_000     # pagination batch size

# ---------------------------------------------------------------------------
# Source codes (match Rust ConceptSource enum ordinals)
# ---------------------------------------------------------------------------
SOURCE_WIKIDATA: int = 0
SOURCE_GEONAMES: int = 1
SOURCE_NCBI: int = 2
SOURCE_CHEBI: int = 3

SOURCE_NAMES: dict[int, str] = {
    SOURCE_WIKIDATA: "wikidata",
    SOURCE_GEONAMES: "geonames",
    SOURCE_NCBI: "ncbi",
    SOURCE_CHEBI: "chebi",
}

# ---------------------------------------------------------------------------
# Category constants (match Rust ConceptCategory enum)
# ---------------------------------------------------------------------------
CATEGORY_ENTITY: int = 0
CATEGORY_PROPERTY: int = 1
CATEGORY_UNIT: int = 2
CATEGORY_TAXON: int = 3
CATEGORY_PLACE: int = 4
CATEGORY_PERSON: int = 5
CATEGORY_EVENT: int = 6
CATEGORY_SUBSTANCE: int = 7
CATEGORY_OTHER: int = 255

# ---------------------------------------------------------------------------
# Wikidata cross-reference properties
# ---------------------------------------------------------------------------
WD_PROP_CHEBI: str = "P683"
WD_PROP_NCBI: str = "P846"
WD_PROP_GEONAMES: str = "P1566"
WD_PROP_CAS: str = "P231"

CROSS_REF_PROPERTIES: list[str] = [
    WD_PROP_CHEBI,
    WD_PROP_NCBI,
    WD_PROP_GEONAMES,
    WD_PROP_CAS,
]

# ---------------------------------------------------------------------------
# OBR binary format constants
# ---------------------------------------------------------------------------
OBR_MAGIC: bytes = b"OBR1"
OBR_VERSION: int = 1

# ---------------------------------------------------------------------------
# P31 category → numeric category mapping
# ---------------------------------------------------------------------------

_P31_TO_CATEGORY: dict[str, int] = {
    "human": CATEGORY_PERSON,
    "city": CATEGORY_PLACE,
    "country": CATEGORY_PLACE,
    "region": CATEGORY_PLACE,
    "taxon": CATEGORY_TAXON,
    "chemical_compound": CATEGORY_SUBSTANCE,
    "occurrence": CATEGORY_EVENT,
    "unit": CATEGORY_UNIT,
    "organization": CATEGORY_ENTITY,
    "disease": CATEGORY_ENTITY,
    "protein": CATEGORY_SUBSTANCE,
    "gene": CATEGORY_ENTITY,
    "software": CATEGORY_ENTITY,
    "language": CATEGORY_ENTITY,
    "film": CATEGORY_ENTITY,
    "book": CATEGORY_ENTITY,
    "sport": CATEGORY_ENTITY,
    "occupation": CATEGORY_PROPERTY,
}


def p31_to_category(p31_label: str) -> int:
    """Map a Wikidata P31 category label to the numeric category code.

    Args:
        p31_label: One of the keys in ``WIKIDATA_P31_CATEGORIES``
                   (e.g. ``"human"``, ``"city"``).

    Returns:
        The corresponding ``CATEGORY_*`` integer, or ``CATEGORY_OTHER``
        if the label is not recognised.
    """
    return _P31_TO_CATEGORY.get(p31_label, CATEGORY_OTHER)
