"""
Review tool for OneBrain Concept Registry (.obr / .jsonl files).

Usage:
    python review.py stats                  # Show statistics
    python review.py search "water"         # Search for a concept
    python review.py sample 20              # Show 20 random records
    python review.py export-csv 1000        # Export first 1000 to CSV
    python review.py verify                 # Check basic concepts exist
"""

import argparse
import csv
import json
import os
import random
import sys
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8")

SCRIPT_DIR = Path(__file__).parent
RAW_DIR = SCRIPT_DIR / "raw"
MERGED_DIR = SCRIPT_DIR / "merged"
OBR_PATH = SCRIPT_DIR.parent.parent / "onebrain_data" / "concepts.obr"

# Basic concepts that MUST exist for 99% coverage
BASIC_CONCEPTS = [
    # Nature
    "water", "fire", "air", "earth", "sun", "moon", "star", "sky",
    "rain", "snow", "wind", "ice", "cloud", "sea", "ocean", "river",
    # Life
    "tree", "flower", "grass", "seed", "fish", "bird", "dog", "cat",
    "horse", "cow", "insect", "snake", "animal", "plant", "human",
    # Body
    "heart", "brain", "blood", "bone", "eye", "hand", "head", "skin",
    # Food
    "food", "bread", "rice", "milk", "egg", "meat", "fruit", "salt",
    # Abstract
    "love", "anger", "fear", "joy", "sadness", "hope", "peace", "war",
    "time", "space", "life", "death", "truth", "freedom", "justice",
    # Properties
    "hot", "cold", "big", "small", "fast", "slow", "old", "new",
    "red", "blue", "green", "white", "black", "light", "dark",
    # Objects
    "house", "car", "book", "door", "table", "chair", "knife", "tool",
    "computer", "phone", "money", "road", "bridge", "wall",
    # Science
    "energy", "force", "gravity", "atom", "cell", "gene", "virus",
    "oxygen", "carbon", "iron", "gold", "copper",
    # Social
    "family", "friend", "child", "mother", "father", "king", "god",
    "language", "music", "art", "science", "history", "law",
]


def cmd_stats(args: argparse.Namespace) -> None:
    """Show statistics about raw JSONL files."""
    print("=" * 60)
    print("CONCEPT REGISTRY STATISTICS")
    print("=" * 60)

    total = 0
    for fname in ["wikidata.jsonl", "geonames.jsonl", "ncbi_taxonomy.jsonl", "chebi.jsonl"]:
        path = RAW_DIR / fname
        if not path.exists():
            print(f"  {fname:25s}  NOT FOUND")
            continue
        count = sum(1 for _ in open(path, encoding="utf-8"))
        size_mb = path.stat().st_size / 1024 / 1024
        print(f"  {fname:25s}  {count:>10,} records  ({size_mb:.1f} MB)")
        total += count

    print(f"  {'TOTAL':25s}  {total:>10,}")

    # Merged
    merged = MERGED_DIR / "concepts_deduped.jsonl"
    if merged.exists():
        count = sum(1 for _ in open(merged, encoding="utf-8"))
        size_mb = merged.stat().st_size / 1024 / 1024
        print(f"\n  Merged (deduped):          {count:>10,} ({size_mb:.1f} MB)")

    # OBR
    if OBR_PATH.exists():
        size_mb = OBR_PATH.stat().st_size / 1024 / 1024
        print(f"  OBR file:                  {size_mb:.1f} MB")


def cmd_search(args: argparse.Namespace) -> None:
    """Search for concepts by name across all raw files."""
    query = args.query.lower()
    print(f'Searching for "{query}" across all sources...\n')

    results = []
    for fname in ["wikidata.jsonl", "geonames.jsonl", "ncbi_taxonomy.jsonl", "chebi.jsonl"]:
        path = RAW_DIR / fname
        if not path.exists():
            continue
        source = fname.split(".")[0]
        with open(path, encoding="utf-8") as f:
            for line in f:
                try:
                    obj = json.loads(line)
                except Exception:
                    continue

                name = obj.get("name", "")
                labels = obj.get("labels", {})

                # Check name
                if name.lower() == query:
                    results.append((source, obj, "exact-name"))
                elif query in name.lower():
                    results.append((source, obj, "partial-name"))
                    continue

                # Check labels
                for lang, label in labels.items():
                    if isinstance(label, str) and label.lower() == query:
                        results.append((source, obj, f"label-{lang}"))
                        break

                if len(results) >= 50:
                    break

    if not results:
        print(f"  No results found for '{query}'")
        return

    # Sort: exact matches first
    results.sort(key=lambda x: (0 if x[2].startswith("exact") else 1))

    print(f"Found {len(results)} results:\n")
    for source, obj, match_type in results[:20]:
        # Build ID string
        if "qid" in obj:
            id_str = f"Q{obj['qid']}"
        elif "geonames_id" in obj:
            id_str = f"GN{obj['geonames_id']}"
        elif "taxid" in obj:
            id_str = f"NCBI{obj['taxid']}"
        elif "chebi_id" in obj:
            id_str = f"CHEBI{obj['chebi_id']}"
        else:
            id_str = "?"

        name = obj.get("name", "?")
        cat = obj.get("category", "?")
        labels = obj.get("labels", {})
        en_label = labels.get("en", "")
        vi_label = labels.get("vi", "")

        print(f"  [{match_type:12s}] {id_str:15s} | {name[:40]:40s} | cat={cat}")
        if en_label and en_label != name:
            print(f"                {'':15s} | en: {en_label}")
        if vi_label:
            print(f"                {'':15s} | vi: {vi_label}")

    if len(results) > 20:
        print(f"\n  ... and {len(results) - 20} more results")


def cmd_sample(args: argparse.Namespace) -> None:
    """Show N random records from merged or raw files."""
    n = args.n

    # Try merged first, then wikidata
    target = MERGED_DIR / "concepts_deduped.jsonl"
    if not target.exists():
        target = RAW_DIR / "wikidata.jsonl"
    if not target.exists():
        print("No data files found!")
        return

    # Read all lines (for large files, reservoir sample)
    lines = []
    with open(target, encoding="utf-8") as f:
        for i, line in enumerate(f):
            if i < n:
                lines.append(line)
            else:
                j = random.randint(0, i)
                if j < n:
                    lines[j] = line

    print(f"Random {len(lines)} records from {target.name}:\n")
    for line in lines:
        try:
            obj = json.loads(line)
            name = obj.get("name", "?")
            labels = obj.get("labels", {})
            en = labels.get("en", "")
            vi = labels.get("vi", "")
            cat = obj.get("category", "?")

            if "qid" in obj:
                id_str = f"Q{obj['qid']}"
            elif "geonames_id" in obj:
                id_str = f"GN{obj['geonames_id']}"
            elif "taxid" in obj:
                id_str = f"NCBI{obj['taxid']}"
            elif "chebi_id" in obj:
                id_str = f"CHEBI{obj['chebi_id']}"
            else:
                id_str = "?"

            print(f"  {id_str:15s} | {(en or name)[:50]:50s} | {cat}")
            if vi:
                print(f"  {'':15s} | vi: {vi}")
        except Exception:
            pass
    print()


def cmd_verify(args: argparse.Namespace) -> None:
    """Verify that basic concepts exist in the registry."""
    print("=" * 60)
    print("BASIC CONCEPT VERIFICATION")
    print(f"Checking {len(BASIC_CONCEPTS)} essential concepts...")
    print("=" * 60)

    # Build index of English labels from wikidata
    found = {}
    path = RAW_DIR / "wikidata.jsonl"
    if not path.exists():
        print("wikidata.jsonl not found!")
        return

    with open(path, encoding="utf-8") as f:
        for line in f:
            try:
                obj = json.loads(line)
                labels = obj.get("labels", {})
                en = labels.get("en", "").lower()
                if en in BASIC_CONCEPTS and en not in found:
                    qid = obj.get("qid", "?")
                    cat = obj.get("category", "?")
                    desc = obj.get("description", "")[:60]
                    found[en] = f"Q{qid} ({cat}) — {desc}"
            except Exception:
                pass

    ok_count = 0
    missing = []
    for concept in BASIC_CONCEPTS:
        if concept in found:
            print(f"  [OK] {concept:15s} → {found[concept]}")
            ok_count += 1
        else:
            print(f"  [XX] {concept:15s} → MISSING!")
            missing.append(concept)

    print()
    pct = ok_count / len(BASIC_CONCEPTS) * 100
    print(f"Coverage: {ok_count}/{len(BASIC_CONCEPTS)} ({pct:.0f}%)")
    if missing:
        print(f"Missing: {', '.join(missing)}")


def cmd_export_csv(args: argparse.Namespace) -> None:
    """Export first N records to CSV."""
    n = args.n
    target = MERGED_DIR / "concepts_deduped.jsonl"
    if not target.exists():
        target = RAW_DIR / "wikidata.jsonl"

    out_path = SCRIPT_DIR / "export.csv"

    with (
        open(target, encoding="utf-8") as f,
        open(out_path, "w", encoding="utf-8", newline="") as csvf,
    ):
        writer = csv.writer(csvf)
        writer.writerow(["id", "name", "en_label", "vi_label", "category", "description"])

        for i, line in enumerate(f):
            if i >= n:
                break
            try:
                obj = json.loads(line)
                labels = obj.get("labels", {})
                writer.writerow([
                    obj.get("qid", obj.get("geonames_id", obj.get("taxid", obj.get("chebi_id", "?")))),
                    obj.get("name", ""),
                    labels.get("en", ""),
                    labels.get("vi", ""),
                    obj.get("category", ""),
                    obj.get("description", "")[:100],
                ])
            except Exception:
                pass

    print(f"Exported {min(n, i+1)} records to {out_path}")


def main() -> None:
    parser = argparse.ArgumentParser(description="OneBrain Concept Registry Review Tool")
    sub = parser.add_subparsers(dest="command")

    sub.add_parser("stats", help="Show statistics")

    p_search = sub.add_parser("search", help="Search for a concept")
    p_search.add_argument("query", help="Search term")

    p_sample = sub.add_parser("sample", help="Show random records")
    p_sample.add_argument("n", type=int, default=20, nargs="?", help="Number of records")

    sub.add_parser("verify", help="Verify basic concepts exist")

    p_export = sub.add_parser("export-csv", help="Export to CSV")
    p_export.add_argument("n", type=int, default=1000, nargs="?", help="Number of records")

    args = parser.parse_args()

    if args.command == "stats":
        cmd_stats(args)
    elif args.command == "search":
        cmd_search(args)
    elif args.command == "sample":
        cmd_sample(args)
    elif args.command == "verify":
        cmd_verify(args)
    elif args.command == "export-csv":
        cmd_export_csv(args)
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
