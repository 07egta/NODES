#!/usr/bin/env python3
"""Report audited curated coverage, not full Grasshopper compatibility."""
import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path

STATUSES = ("VERIFIED", "IMPLEMENTED", "PARTIAL", "APPROXIMATE",
            "MISSING", "BLOCKED", "UNKNOWN")
MAPPED = set(STATUSES[:4])
PRIORITY = {"Params": 1, "Maths": 1, "Sets": 1, "Data": 1,
            "Vector": 1, "Curve": 2, "Surface": 2, "Mesh": 2,
            "Intersect": 2, "Transform": 2, "Display": 3}

def summarize(rows):
    counts = Counter(row["status"] for row in rows)
    if set(counts) - set(STATUSES):
        raise ValueError("Unknown status")
    if not rows:
        raise ValueError("Empty catalog")
    for row in rows:
        if row["status"] in MAPPED and not row.get("nodes_id"):
            raise ValueError("Mapped status without node ID")
    total = len(rows)
    return {
        "total": total,
        "counts": {s: counts[s] for s in STATUSES},
        "mapped_percent": round(100 * sum(counts[s] for s in MAPPED) / total, 2),
        "verified_percent": round(100 * counts["VERIFIED"] / total, 2),
    }

def family(rows):
    groups = defaultdict(list)
    seen = set()
    for row in rows:
        key = (row["category"], row["name"])
        if key in seen:
            raise ValueError("Duplicate component: " + str(key))
        seen.add(key)
        groups[row["category"]].append(row)
    backlog = [
        {"category": r["category"], "name": r["name"], "status": r["status"],
         "priority_tier": PRIORITY.get(r["category"], 3),
         "limitation": r["limitation"]}
        for r in rows if r["status"] != "VERIFIED"
    ]
    backlog.sort(key=lambda r: (r["priority_tier"],
                 STATUSES.index(r["status"]) < 4, r["category"], r["name"]))
    return {"overall": summarize(rows),
            "categories": {k: summarize(v) for k, v in sorted(groups.items())},
            "backlog": backlog}

def report(catalog):
    if catalog["schema_version"] != 1:
        raise ValueError("Unsupported catalog schema")
    return {
        "scope": "Curated candidates only; mapped includes partial and approximate",
        "baseline_commit": catalog["baseline_commit"],
        "GH1": family(catalog["components"]),
        "GH2": family(catalog["gh2"]["components"]),
        "GH2_published_index_count": catalog["gh2"]["published_index_count"],
        "full_standard_coverage_percent": None,
        "priority_note": "Heuristic category tier; not measured usage frequency",
    }

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--catalog", type=Path, default=Path(__file__).resolve().parents[1]
                        / "docs/compatibility/gh_catalog.json")
    args = parser.parse_args()
    print(json.dumps(report(json.loads(args.catalog.read_text(encoding="utf-8"))),
                     ensure_ascii=False, indent=2))

if __name__ == "__main__":
    main()
