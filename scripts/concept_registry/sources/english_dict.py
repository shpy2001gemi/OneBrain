"""
English Dictionary / Vocabulary source for OneBrain Concept Registry.

Uses NLTK's WordNet to extract words, phrases (multi-word expressions),
and their definitions, POS tags, synonyms, and hypernyms.

Output: JSONL with one record per unique lemma.
"""

import json
import logging
from pathlib import Path
from collections import defaultdict

from tqdm import tqdm

import nltk
from nltk.corpus import wordnet as wn

logger = logging.getLogger(__name__)

# WordNet POS tag mapping
POS_MAP = {
    wn.NOUN: "noun",
    wn.VERB: "verb",
    wn.ADJ: "adj",
    wn.ADV: "adv",
}


def _ensure_nltk_data():
    """Download required NLTK data if not already present."""
    for resource in ("wordnet", "omw-1.4"):
        try:
            nltk.data.find(f"corpora/{resource}")
            logger.debug("NLTK resource '%s' already available.", resource)
        except LookupError:
            logger.info("Downloading NLTK resource '%s'...", resource)
            nltk.download(resource, quiet=True)


def _collect_lemma_data() -> dict:
    """Iterate all WordNet synsets and collect per-lemma data.

    Returns a dict keyed by lemma name (lowercase, with underscores) where
    each value is a dict with keys: definitions, pos, synonyms, hypernyms.
    """
    lemma_data: dict[str, dict] = defaultdict(
        lambda: {
            "definitions": [],
            "pos": set(),
            "synonyms": set(),
            "hypernyms": set(),
        }
    )

    all_synsets = list(wn.all_synsets())
    logger.info("Processing %s synsets from WordNet...", f"{len(all_synsets):,}")

    for synset in tqdm(all_synsets, desc="Scanning synsets", unit="synset"):
        # Extract synset-level info
        definition = synset.definition()
        pos_tag = POS_MAP.get(synset.pos(), synset.pos())
        lemma_names = [lemma.name().lower() for lemma in synset.lemmas()]

        # Hypernyms: first lemma name of each hypernym synset
        hypernym_names = []
        for hyper_synset in synset.hypernyms():
            hyper_lemmas = hyper_synset.lemmas()
            if hyper_lemmas:
                hypernym_names.append(hyper_lemmas[0].name().lower())

        # Update each lemma in this synset
        for lemma_name in lemma_names:
            entry = lemma_data[lemma_name]
            if definition:
                entry["definitions"].append(definition)
            entry["pos"].add(pos_tag)

            # Synonyms = other lemmas in the same synset (excluding self)
            for other in lemma_names:
                if other != lemma_name:
                    entry["synonyms"].add(other.replace("_", " "))

            for hyper in hypernym_names:
                entry["hypernyms"].add(hyper.replace("_", " "))

    return lemma_data


def fetch_all(output_path: Path, checkpoint_dir: Path) -> int:
    """Fetch English dictionary entries from WordNet.

    Args:
        output_path: Path to the output JSONL file.
        checkpoint_dir: Directory for checkpoints (unused for WordNet,
                        kept for API compatibility with other sources).

    Returns:
        Number of entries written.
    """
    _ensure_nltk_data()

    # Collect all lemma data from WordNet
    lemma_data = _collect_lemma_data()

    # Write JSONL output
    output_path.parent.mkdir(parents=True, exist_ok=True)
    count = 0

    logger.info("Writing %s unique lemmas to %s", f"{len(lemma_data):,}", output_path)

    with open(output_path, "w", encoding="utf-8") as f:
        for lemma_name in tqdm(
            sorted(lemma_data.keys()), desc="Writing entries", unit="entry"
        ):
            entry = lemma_data[lemma_name]

            # Determine category
            category = "phrase" if "_" in lemma_name else "word"

            # Build human-readable label (replace underscores with spaces)
            label = lemma_name.replace("_", " ")

            # Combine definitions with '; ' separator
            description = "; ".join(entry["definitions"]) if entry["definitions"] else ""

            record = {
                "id": f"en:{lemma_name}",
                "labels": {"en": label},
                "description": description,
                "category": category,
                "pos": sorted(entry["pos"]),
                "synonyms": sorted(entry["synonyms"]),
                "hypernyms": sorted(entry["hypernyms"]),
                "source": "wordnet",
            }

            f.write(json.dumps(record, ensure_ascii=False) + "\n")
            count += 1

    logger.info("Done. Wrote %s entries.", f"{count:,}")
    return count


if __name__ == "__main__":
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )
    script_dir = Path(__file__).parent.parent
    output = script_dir / "raw" / "english_dict.jsonl"
    output.parent.mkdir(parents=True, exist_ok=True)
    count = fetch_all(output, script_dir / "checkpoints")
    print(f"Wrote {count:,} entries to {output}")
