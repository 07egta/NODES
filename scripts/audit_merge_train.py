#!/usr/bin/env python3
"""Read-only merge-train inventory. Developer tooling only; no app dependency."""
import argparse
import collections
import json
import os
import re
import sys
import urllib.request

SHA = re.compile(r"[0-9a-f]{40}")
REPO = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+")


def pages(repo, endpoint, token):
    for page in range(1, 101):
        sep = "&" if "?" in endpoint else "?"
        url = f"https://api.github.com/repos/{repo}/{endpoint}{sep}per_page=100&page={page}"
        headers = {"Accept": "application/vnd.github+json", "User-Agent": "nodes-merge-audit"}
        if token:
            headers["Authorization"] = "Bearer " + token
        with urllib.request.urlopen(urllib.request.Request(url, headers=headers), timeout=30) as response:
            data = json.load(response)
        rows = data["workflow_runs"] if isinstance(data, dict) else data
        yield from rows
        if len(rows) < 100:
            return
    raise RuntimeError("pagination limit reached; inventory incomplete")


def classify(pr, runs):
    # A PR run's head_sha alone does NOT identify its base or tested merge tree.
    number, head, base = pr["number"], pr["head"]["sha"], pr["base"]["sha"]
    related = [(r, p) for r in runs for p in r.get("pull_requests", [])
               if p["number"] == number]
    exact = [r for r, p in related
             if p["head"]["sha"] == head and p["base"]["sha"] == base]
    if not exact:
        return {"state": "stale" if related else "unknown"}
    run = max(exact, key=lambda r: (r["id"], r.get("run_attempt", 1)))
    return {"state": run["conclusion"] or run["status"], "run": run["id"],
            "note": "Pair match only; inspect exact candidate summary and required jobs"}


def inventory(prs, files, runs):
    branches = {(p["head"]["repo"]["full_name"], p["head"]["ref"]): p["number"]
                for p in prs if p["head"].get("repo")}
    owners = collections.defaultdict(list)
    for n, paths in files.items():
        for path in set(paths):
            owners[path].append(n)
    overlaps = collections.defaultdict(list)
    for path, numbers in owners.items():
        for i, a in enumerate(sorted(numbers)):
            for b in sorted(numbers)[i + 1:]:
                overlaps[(a, b)].append(path)
    deps = {}
    for p in prs:
        base = p["base"]
        deps[p["number"]] = branches.get((base["repo"]["full_name"], base["ref"]))
    # Topological order applies only to proven stacked branches, not file overlap.
    remaining = set(deps)
    order = []
    while remaining:
        available = sorted(n for n in remaining if deps[n] not in remaining)
        if not available:
            break
        order.extend(available)
        remaining.difference_update(available)
    return {
        "prs": [{"number": p["number"], "draft": p["draft"],
                 "base_ref": p["base"]["ref"], "base_sha": p["base"]["sha"],
                 "head_sha": p["head"]["sha"], "stacked_on": deps[p["number"]],
                 "ci": classify(p, runs)} for p in sorted(prs, key=lambda p: p["number"])],
        "stacked_order_not_merge_approval": order,
        "cyclic_or_blocked": sorted(remaining),
        "file_overlap_not_dependency": [{"a": a, "b": b, "paths": sorted(paths),
                                         "same_file_set": set(files[a]) == set(files[b])}
                                        for (a, b), paths in sorted(overlaps.items())],
    }


def self_test():
    def pr(n, base_ref, head_ref):
        return {"number": n, "draft": False,
                "base": {"ref": base_ref, "sha": "a" * 40, "repo": {"full_name": "x/y"}},
                "head": {"ref": head_ref, "sha": "b" * 40, "repo": {"full_name": "x/y"}}}
    a, b = pr(2, "main", "feature"), pr(1, "feature", "fix")
    run = {"id": 1, "conclusion": "success", "status": "completed",
           "pull_requests": [{"number": 2, "head": {"sha": "b" * 40},
                              "base": {"sha": "c" * 40}}]}
    data = inventory([b, a], {1: ["model.rs"], 2: ["model.rs"]}, [run])
    assert data["stacked_order_not_merge_approval"] == [2, 1]
    assert data["prs"][1]["ci"]["state"] == "stale"
    assert data["file_overlap_not_dependency"][0]["same_file_set"]
    run["pull_requests"][0]["base"]["sha"] = "a" * 40
    assert classify(a, [run])["state"] == "success"
    assert classify(b, [run])["state"] == "unknown"
    assert inventory([pr(1, "two", "one"), pr(2, "one", "two")],
                     {1: [], 2: []}, [])["cyclic_or_blocked"] == [1, 2]
    print("merge audit self-test passed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default="07egta/NODES")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    if not REPO.fullmatch(args.repo):
        parser.error("expected owner/repo")
    token = os.environ.get("GH_TOKEN")
    prs = list(pages(args.repo, "pulls?state=open", token))
    files = {}
    for p in prs:
        n = p["number"]
        rows = list(pages(args.repo, f"pulls/{n}/files", token))
        if (p.get("changed_files") is not None and len(rows) != p["changed_files"]) or len(rows) >= 3000:
            raise RuntimeError(f"PR {n} file listing incomplete or changed")
        files[n] = [path for row in rows for path in
                    (row["filename"], row.get("previous_filename")) if path]
        for ref in ("base", "head"):
            if not SHA.fullmatch(p[ref]["sha"]):
                raise RuntimeError(f"PR {n} invalid SHA")
    runs = list(pages(args.repo, "actions/workflows/windows.yml/runs?event=pull_request", token))
    # Reject a moving snapshot rather than mix old file lists with new commits.
    final = list(pages(args.repo, "pulls?state=open", token))
    def keys(items):
        return sorted((p["number"], p["base"]["sha"], p["head"]["sha"],
                       p["base"]["ref"], p["draft"]) for p in items)
    if keys(prs) != keys(final):
        raise RuntimeError("PR snapshot changed; rerun inventory")
    print(json.dumps(inventory(prs, files, runs), ensure_ascii=False, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, RuntimeError) as exc:
        sys.exit(str(exc))
