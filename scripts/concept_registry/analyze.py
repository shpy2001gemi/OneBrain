"""Analyze Wikidata data quality and OBR size."""
import sys
import json
import os
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8")

SCRIPT_DIR = Path(__file__).parent
RAW_DIR = SCRIPT_DIR / "raw"
OBR_PATH = SCRIPT_DIR.parent.parent / "onebrain_data" / "concepts.obr"

# 1. Wikidata category distribution
print("=" * 60)
print("WIKIDATA CATEGORY DISTRIBUTION")
print("=" * 60)
cats = {}
total = 0
with open(RAW_DIR / "wikidata.jsonl", "r", encoding="utf-8") as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        try:
            obj = json.loads(line)
            cat = obj.get("category", "?")
            cats[cat] = cats.get(cat, 0) + 1
            total += 1
        except Exception:
            pass

for k, v in sorted(cats.items(), key=lambda x: -x[1]):
    pct = v / total * 100
    print(f"  {k:25s} {v:>8,}  ({pct:.1f}%)")
print(f"  {'TOTAL':25s} {total:>8,}")

# 2. Search for basic concepts
print()
print("=" * 60)
print("BASIC CONCEPT SEARCH (should ALL exist in a good registry)")
print("=" * 60)
targets = [
    "water", "fish", "anger", "hot", "cold", "love", "tree",
    "sun", "moon", "fire", "earth", "air", "food", "dog", "cat",
    "house", "car", "book", "music", "time", "death", "life",
    "red", "blue", "green", "big", "small", "happy", "sad",
]
found = {}
with open(RAW_DIR / "wikidata.jsonl", "r", encoding="utf-8") as f:
    for line in f:
        try:
            obj = json.loads(line)
            labels = obj.get("labels", {})
            en = labels.get("en", "").lower()
            if en in targets:
                found[en] = f"Q{obj['qid']} ({obj.get('category', '?')})"
        except Exception:
            pass

for t in targets:
    status = found.get(t, "MISSING!")
    mark = "OK" if t in found else "XX"
    print(f"  [{mark}] {t:12s} -> {status}")

missing = [t for t in targets if t not in found]
print(f"\n  Found: {len(found)}/{len(targets)}, Missing: {len(missing)}")

# 3. OBR size analysis
print()
print("=" * 60)
print("OBR SIZE ANALYSIS")
print("=" * 60)

obr_size = os.path.getsize(OBR_PATH)
print(f"  OBR file: {obr_size / 1024 / 1024:.1f} MB")

# Estimate compressed size
import gzip
with open(OBR_PATH, "rb") as f:
    sample = f.read(10 * 1024 * 1024)  # 10MB sample
    compressed = gzip.compress(sample, compresslevel=6)
    ratio = len(compressed) / len(sample)
    print(f"  Compression ratio (gzip): {ratio:.1%}")
    print(f"  Estimated compressed: {obr_size * ratio / 1024 / 1024:.1f} MB")
