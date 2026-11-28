#!/usr/bin/env python3
"""Read-only coordination snapshot; requires authenticated gh, not at runtime."""
import argparse
import datetime as dt
import itertools
import json
import re
import subprocess
import sys

SHA = re.compile(r"[0-9a-f]{40}")
BLOCK = re.compile(r"(?m)^\x60\x60\x60nodes-coordination\s*\n(.*?)\n\x60\x60\x60\s*$", re.S | re.M)
def api(repo, endpoint, pages=False):
    args = ["gh", "api", "--method", "GET", f"repos/{repo}/{endpoint}"]
    if pages:
        args += ["--paginate", "--slurp"]
    raw = subprocess.run(args, capture_output=True, text=True, check=True, timeout=120)
    value = json.loads(raw.stdout)
    return list(itertools.chain.from_iterable(value)) if pages else value

def claim(body):
    blocks = BLOCK.findall(body or "")
    if len(blocks) != 1:
        return None, "missing or multiple declarations"
    try:
        c = json.loads(blocks[0])
        if not isinstance(c, dict) or c.get("version") != 1:
            raise ValueError("version")
        for k in ("mission", "task", "handoff"):
            if not isinstance(c.get(k), str) or not c[k].strip():
                raise ValueError(k)
        if not SHA.fullmatch(c.get("base_sha", "")):
            raise ValueError("base_sha")
        if c.get("verified_sha") is not None and not SHA.fullmatch(c["verified_sha"]):
            raise ValueError("verified_sha")
        if c.get("state") not in ("active", "blocked", "review", "handoff"):
            raise ValueError("state")
        if c.get("priority") not in ("P0", "P1", "P2", "P3"):
            raise ValueError("priority")
        for k in ("issues", "depends_on"):
            if not isinstance(c.get(k), list) or any(type(n) is not int or n <= 0 for n in c[k]):
                raise ValueError(k)
        if not isinstance(c.get("scope"), list) or not c["scope"]:
            raise ValueError("scope")
        for p in c["scope"]:
            if not isinstance(p, str) or not p or p.startswith("/") or any(x in p for x in ("*", "\\", "?", "[", "]")) or any(x in ("", ".", "..") for x in p.rstrip("/").split("/")):
                raise ValueError("scope path")
        stamp = dt.datetime.fromisoformat(c["checked_at"].replace("Z", "+00:00"))
        if stamp.utcoffset() is None:
            raise ValueError("checked_at timezone")
        return c, None
    except (ValueError, TypeError, KeyError, AttributeError) as exc:
        return None, f"invalid declaration: {exc}"

def overlaps(a, b):
    return a == b or (a.endswith("/") and b.startswith(a)) or (b.endswith("/") and a.startswith(b))

def conflicts(rows):
    result = []
    for a, b in itertools.combinations(rows, 2):
        ac, bc = a["claim"] or {}, b["claim"] or {}
        af, bf = set(a["files"]), set(b["files"])
        sa, sb = set(ac.get("scope", [])), set(bc.get("scope", []))
        shared = sorted(af & bf)
        planned = sorted({(x, y) for x in sa | af for y in sb | bf if overlaps(x, y) and not (x == y and x in af & bf)})
        issues = sorted(set(ac.get("issues", [])) & set(bc.get("issues", [])))
        same_task = bool(ac and bc and ac["task"] == bc["task"])
        if shared or planned or issues or same_task:
            result.append({"prs": [a["number"], b["number"]], "files": shared,
                           "scope_overlap": planned, "issues": issues, "same_task": same_task,
                           "declared_dependency": a["number"] in bc.get("depends_on", []) or b["number"] in ac.get("depends_on", [])})
    return result

def snapshot(repo):
    now = dt.datetime.now(dt.timezone.utc)
    base = api(repo, "git/ref/heads/main")["object"]["sha"]
    prs = api(repo, "pulls?state=open&per_page=100", True)
    issues = [i for i in api(repo, "issues?state=open&per_page=100", True) if "pull_request" not in i]
    rows, errors = [], []
    for pr in sorted(prs, key=lambda p: p["number"]):
        n, head = pr["number"], pr["head"]["sha"]
        c, error = claim(pr.get("body"))
        row = {"number": n, "title": pr["title"], "branch": pr["head"]["ref"], "head_sha": head,
               "base_ref": pr["base"]["ref"], "draft": pr["draft"], "updated_at": pr["updated_at"],
               "claim": c, "claim_error": error, "files": [], "warnings": [], "ci": None}
        try:
            files = api(repo, f"pulls/{n}/files?per_page=100", True)
            row["files"] = sorted({p for f in files for p in (f["filename"], f.get("previous_filename")) if p})
            if len(files) >= 3000:
                row["warnings"].append("file API cap reached; scope incomplete")
                errors.append(f"#{n}: file API cap")
            check = api(repo, f"commits/{head}/check-runs?per_page=100")
            row["ci"] = [{"name": x["name"], "status": x["status"], "conclusion": x["conclusion"]}
                         for x in check["check_runs"] if x["head_sha"] == head]
            if check["total_count"] > len(check["check_runs"]):
                errors.append(f"#{n}: check list incomplete")
            row["combined_status"] = api(repo, f"commits/{head}/status")["state"]
            current = api(repo, f"pulls/{n}")
            if current["head"]["sha"] != head or current["state"] != "open" or current.get("body") != pr.get("body"):
                row["warnings"].append("PR changed during scan")
                errors.append(f"#{n}: changed during scan")
        except (subprocess.SubprocessError, OSError, ValueError, KeyError) as exc:
            errors.append(f"#{n}: {type(exc).__name__}")
        if c:
            if c["base_sha"] != base:
                row["warnings"].append("declared base differs from main; recheck")
            if c["verified_sha"] != head:
                row["warnings"].append("current head not declared verified")
            stamp = dt.datetime.fromisoformat(c["checked_at"].replace("Z", "+00:00"))
            if stamp > now or now - stamp > dt.timedelta(hours=24):
                row["warnings"].append("claim timestamp needs refresh")
            outside = [p for p in row["files"] if not any(overlaps(p, s) for s in c["scope"])]
            if outside:
                row["warnings"].append({"undeclared_files": outside})
        else:
            row["warnings"].append("legacy or invalid declaration; inspect manually")
        rows.append(row)
    final_base = api(repo, "git/ref/heads/main")["object"]["sha"]
    final_prs = api(repo, "pulls?state=open&per_page=100", True)
    fingerprint = lambda ps: sorted((p["number"], p["head"]["sha"], p["updated_at"]) for p in ps)
    if base != final_base or fingerprint(prs) != fingerprint(final_prs):
        errors.append("main or open PR inventory changed during scan; rerun")
    claimed = {n for r in rows for n in (r["claim"] or {}).get("issues", [])}
    return {"checked_at": now.isoformat(), "main_sha": base, "complete": not errors,
            "errors": errors, "prs": rows, "overlaps": conflicts(rows),
            "issues_without_structured_claim": [{"number": i["number"], "title": i["title"]}
                                               for i in issues if i["number"] not in claimed],
            "note": "Unclaimed is not unimplemented. Check legacy PRs and history. Overlap is not a merge conflict. CI is evidence only, not merge approval."}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default="07egta/NODES")
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", args.repo):
        parser.error("expected owner/repo")
    try:
        report = snapshot(args.repo)
    except (subprocess.SubprocessError, OSError, ValueError, KeyError) as exc:
        print(f"snapshot failed: {type(exc).__name__}", file=sys.stderr)
        return 2
    print(json.dumps(report, ensure_ascii=True, indent=2))
    return 0 if report["complete"] else 2

if __name__ == "__main__":
    sys.exit(main())
