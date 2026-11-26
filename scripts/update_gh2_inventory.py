#!/usr/bin/env python3
"""Collect only the public GH2 toolbar index; never fetch individual components.

Python 3.10+ standard library only. No Rhino, SDK, authentication, or scripts from
the source site are loaded. The output is deterministic for identical HTML.

    python scripts/update_gh2_inventory.py
    python scripts/update_gh2_inventory.py --source-html saved-index.html --check
    python scripts/update_gh2_inventory.py --self-test

Use --expect-count / --expect-version to pin an archived export. --source-html
accepts a previously downloaded copy of the same official index for offline use.
The source SHA-256 and exporter metadata identify the exact snapshot. UNKNOWN
means unresearched: this file never infers implementation or compatibility from
component names, URLs, categories, or the NODES registry.
"""

from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime
import hashlib
from html.parser import HTMLParser
import json
import os
from pathlib import Path
import re
import sys
import tempfile
import unittest
from urllib.parse import urljoin, urlsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener


SOURCE_URL = "https://www.rhino3d.com/docs/grasshopper2/specs/"
MAX_SOURCE_BYTES = 2 * 1024 * 1024
DEFAULT_OUTPUT = (
    Path(__file__).resolve().parents[1]
    / "docs/compatibility/gh2_official_inventory.json"
)
SPEC_URL = re.compile(
    r"https://www\.rhino3d\.com/docs/grasshopper2/specs/([0-9a-fA-F]{32})\.html"
)
GENERATOR = re.compile(
    r"GH2DocsSiteExport; rhino (?P<version>\d+(?:\.\d+){3}); "
    r"rhino-commit (?P<rhino_commit>[0-9a-f]{40}); "
    r"grasshopper2-commit (?P<gh2_commit>[0-9a-f]{40}); "
    r"exported (?P<exported_at>\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z)"
)


def normalized(text: str) -> str:
    return " ".join(text.split())


class OfficialRedirectsOnly(HTTPRedirectHandler):
    """Do not follow the fixed public request to another host or protocol."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        target = urlsplit(newurl)
        if (
            target.scheme != "https"
            or target.netloc != "www.rhino3d.com"
            or target.path not in (
                "/docs/grasshopper2/specs/",
                "/docs/grasshopper2/specs/index.html",
            )
            or target.query
            or target.fragment
        ):
            raise ValueError("The official index redirected outside its expected location")
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def fetch_index() -> bytes:
    request = Request(
        SOURCE_URL,
        headers={
            "User-Agent": "NODES-CompatibilityInventory/1.0 (public documentation research)",
            "Accept": "text/html",
        },
    )
    with build_opener(OfficialRedirectsOnly()).open(request, timeout=30) as response:
        if response.status != 200:
            raise ValueError(f"Official index returned HTTP {response.status}")
        if response.headers.get_content_type() != "text/html":
            raise ValueError("Official index did not return HTML")
        data = response.read(MAX_SOURCE_BYTES + 1)
    if len(data) > MAX_SOURCE_BYTES:
        raise ValueError("Official index exceeds the 2 MiB download limit")
    return data


class IndexParser(HTMLParser):
    """Read the index article's headings and links, not sidebar/search content."""

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.generator: str | None = None
        self.inside = False
        self.seen_article = False
        self.capture: str | None = None
        self.text: list[str] = []
        self.lede: str | None = None
        self.category: str | None = None
        self.subcategory: str | None = None
        self.in_list_item = False
        self.authored = False
        self.link: str | None = None
        self.entries: list[dict] = []

    def begin_capture(self, tag: str) -> None:
        if self.capture is not None:
            raise ValueError("Unexpected nested index heading or component link")
        self.capture = tag
        self.text = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        attrs = dict(attrs)
        classes = (attrs.get("class") or "").split()
        if tag == "meta" and attrs.get("name") == "generator":
            if self.generator is not None:
                raise ValueError("Duplicate exporter metadata")
            self.generator = attrs.get("content") or ""
        if tag == "article" and "index-page" in classes:
            if self.seen_article:
                raise ValueError("Duplicate component index article")
            self.seen_article = self.inside = True
        if not self.inside:
            return
        if tag == "h2" and "alpha-head" in classes:
            self.begin_capture(tag)
        elif tag == "h3" and "sub-head" in classes:
            self.begin_capture(tag)
        elif tag == "p" and "lede" in classes:
            self.begin_capture(tag)
        elif tag == "li":
            if self.in_list_item:
                raise ValueError("Unexpected nested component list item")
            self.in_list_item = True
            self.authored = "authored" in classes
        elif tag == "a" and self.in_list_item:
            self.link = urljoin(SOURCE_URL, attrs.get("href") or "")
            if SPEC_URL.fullmatch(self.link) is None:
                raise ValueError(f"Unexpected component URL: {self.link}")
            self.begin_capture(tag)

    def handle_data(self, data: str) -> None:
        if self.capture is not None:
            self.text.append(data)

    def handle_endtag(self, tag: str) -> None:
        if tag == self.capture:
            text = normalized("".join(self.text))
            if not text:
                raise ValueError(f"Empty index {tag}")
            if tag == "h2":
                self.category = text
                self.subcategory = None
            elif tag == "h3":
                if self.category is None:
                    raise ValueError("Subcategory without a category")
                self.subcategory = text
            elif tag == "p":
                if self.lede is not None:
                    raise ValueError("Duplicate component count paragraph")
                self.lede = text
            elif tag == "a":
                if not self.category or not self.subcategory or not self.link:
                    raise ValueError("Component link without toolbar category metadata")
                identifier = SPEC_URL.fullmatch(self.link).group(1).lower()
                self.entries.append({
                    "public_id": identifier,
                    "name": text,
                    "category": self.category,
                    "subcategory": self.subcategory,
                    "source_url": self.link,
                    "has_in_depth_article": self.authored,
                    "entity_kind": None,
                    "input_ports": None,
                    "output_ports": None,
                    "data_types": None,
                    "default_values": None,
                    "mathematical_meaning": None,
                    "data_matching_rules": None,
                    "nodes_node_id": None,
                    "implementation_status": "UNKNOWN",
                    "test_status": "UNKNOWN",
                    "compatibility_status": "UNKNOWN",
                })
                self.link = None
            self.capture = None
            self.text = []
        if tag == "li" and self.inside:
            self.in_list_item = self.authored = False
        if tag == "article" and self.inside:
            if self.capture is not None or self.in_list_item:
                raise ValueError("Index article ended before its entries were complete")
            self.inside = False


def inventory(raw: bytes) -> dict:
    if not raw or len(raw) > MAX_SOURCE_BYTES:
        raise ValueError("Empty or oversized index HTML")
    parser = IndexParser()
    parser.feed(raw.decode("utf-8-sig"))
    parser.close()
    if not parser.seen_article or parser.inside or parser.capture or parser.in_list_item:
        raise ValueError("Missing or incomplete official index article")
    generator = GENERATOR.fullmatch(parser.generator or "")
    if generator is None:
        raise ValueError("Missing or changed GH2 exporter provenance; review the source first")
    provenance = generator.groupdict()
    datetime.strptime(provenance["exported_at"], "%Y-%m-%dT%H:%M:%SZ")
    count = re.search(r"\b(\d+) components,", parser.lede or "")
    articles = re.search(r"\b(\d+) have an in-depth article", parser.lede or "")
    if count is None or articles is None:
        raise ValueError("Missing published component or in-depth article count")
    expected_count, expected_articles = int(count.group(1)), int(articles.group(1))
    entries = parser.entries
    if len(entries) != expected_count or not entries:
        raise ValueError(f"Collected {len(entries)} entries; official index declares {expected_count}")
    if len({entry["public_id"] for entry in entries}) != len(entries):
        raise ValueError("Duplicate public component IDs; do not silently merge entries")
    if sum(entry["has_in_depth_article"] for entry in entries) != expected_articles:
        raise ValueError("In-depth article markers disagree with the published count")
    categories = Counter(entry["category"] for entry in entries)
    subcategories: dict[str, dict[str, int]] = {}
    for entry in entries:
        group = subcategories.setdefault(entry["category"], {})
        group[entry["subcategory"]] = group.get(entry["subcategory"], 0) + 1
    return {
        "schema_version": 1,
        "product_family": "GH2",
        "scope": "All component and parameter links in the official GH2 toolbar index",
        "generator": "scripts/update_gh2_inventory.py",
        "source": {
            "url": SOURCE_URL,
            "rhino_gh2_version": provenance["version"],
            "rhino_source_commit": provenance["rhino_commit"],
            "grasshopper2_source_commit": provenance["gh2_commit"],
            "exported_at_utc": provenance["exported_at"],
            "html_sha256": hashlib.sha256(raw).hexdigest(),
            "html_bytes": len(raw),
            "metadata_basis": "Official index HTML generator meta tag, toolbar headings and links",
            "public_id_basis": "32-hex identifier in the official documentation filename; not a GH1 mapping",
            "published_entry_count": expected_count,
            "published_in_depth_article_count": expected_articles,
        },
        "inventory_count": len(entries),
        "category_counts": dict(categories),
        "subcategory_counts": subcategories,
        "status_counts": {"UNKNOWN": len(entries)},
        "evidence_policy": {
            "meaning_of_unknown": "Specification and NODES mapping have not been researched in this inventory",
            "null_fields": "Unknown, not empty ports or absence of defaults",
            "individual_component_pages_fetched": 0,
            "gh_reference_evaluations_performed": 0,
            "nodes_registry_used_as_evidence": False,
            "implementation_inferred_from_names": False,
            "limitations": [
                "Inventory completeness applies only to the identified GH2 documentation export",
                "Does not enumerate GH1, third-party plugins, or undocumented components",
                "Entity kind, ports, types, defaults, mathematics and matching need per-entry research",
                "Official index presence is not evidence of NODES implementation or compatibility",
                "In-depth article markers describe documentation availability, not verification",
                "No individual documentation bodies, icons, SDK binaries or implementation code are copied",
            ],
        },
        "entries": entries,
    }


def serialize(data: dict) -> str:
    return json.dumps(data, ensure_ascii=False, indent=2) + "\n"


class InventoryTests(unittest.TestCase):
    def fixture(self) -> bytes:
        return (
            '<meta name="generator" content="GH2DocsSiteExport; rhino 9.3.26329.6001; '
            f'rhino-commit {"a" * 40}; grasshopper2-commit {"b" * 40}; '
            'exported 2026-11-25T15:23:50Z">'
            '<aside><a href="../specs/00000000000000000000000000000009.html">Ignore</a></aside>'
            '<article class="content index-page"><p class="lede">2 components, grouped as in '
            'the toolbar. <strong>1</strong> have an in-depth article.</p>'
            '<h2 class="alpha-head">Data</h2><h3 class="sub-head">List</h3><ul>'
            '<li class="authored"><a href="../specs/00000000000000000000000000000001.html">'
            'A &amp; B</a><span class="authored-mark">●</span></li></ul>'
            '<h2 class="alpha-head">Params</h2><h3 class="sub-head">Primitive</h3><ul>'
            '<li><a href="../specs/00000000000000000000000000000002.html">Number</a></li>'
            '</ul></article>'
        ).encode()

    def test_complete_counts_categories_names_and_no_inferred_compatibility(self):
        data = inventory(self.fixture())
        self.assertEqual(data["inventory_count"], 2)
        self.assertEqual(data["category_counts"], {"Data": 1, "Params": 1})
        self.assertEqual(data["source"]["exported_at_utc"], "2026-11-25T15:23:50Z")
        self.assertEqual(data["entries"][0]["name"], "A & B")
        for entry in data["entries"]:
            self.assertEqual(entry["implementation_status"], "UNKNOWN")
            self.assertEqual(entry["compatibility_status"], "UNKNOWN")
            for key in ("entity_kind", "input_ports", "output_ports", "default_values", "nodes_node_id"):
                self.assertIsNone(entry[key])
        self.assertEqual(serialize(data), serialize(inventory(self.fixture())))

    def test_reject_incomplete_or_changed_source(self):
        for raw in (
            self.fixture().replace(b"2 components,", b"3 components,"),
            self.fixture().replace(b"1</strong> have", b"2</strong> have"),
            self.fixture().replace(b"</article>", b""),
            self.fixture().replace(b"GH2DocsSiteExport", b"UnknownExporter"),
            self.fixture().replace(b'<h3 class="sub-head">List</h3>', b""),
        ):
            with self.subTest(raw=raw[-100:]), self.assertRaises(ValueError):
                inventory(raw)

    def test_reject_duplicate_ids_and_external_links(self):
        with self.assertRaises(ValueError):
            inventory(self.fixture().replace(b"00000000000000000000000000000002", b"00000000000000000000000000000001"))
        with self.assertRaises(ValueError):
            inventory(self.fixture().replace(b'href="../specs/00000000000000000000000000000002.html"', b'href="https://example.com/component"'))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--source-html", type=Path, help="Read a saved official index instead of HTTP")
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--check", action="store_true", help="Compare with the existing inventory; do not write")
    parser.add_argument("--expect-count", type=int)
    parser.add_argument("--expect-version")
    parser.add_argument("--self-test", action="store_true", help="Run offline parser regressions only")
    args = parser.parse_args()
    if args.self_test:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(InventoryTests)
        return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1
    try:
        if args.source_html:
            with args.source_html.open("rb") as stream:
                raw = stream.read(MAX_SOURCE_BYTES + 1)
        else:
            raw = fetch_index()
        result = inventory(raw)
        if args.expect_count is not None and result["inventory_count"] != args.expect_count:
            raise ValueError("Official entry count differs from --expect-count")
        if args.expect_version and result["source"]["rhino_gh2_version"] != args.expect_version:
            raise ValueError("Official export version differs from --expect-version")
        rendered = serialize(result)
        if args.check:
            if args.output.read_text(encoding="utf-8") != rendered:
                raise ValueError("Inventory differs; inspect source changes before regeneration")
        else:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            temporary = None
            try:
                with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", newline="\n", dir=args.output.parent, delete=False) as stream:
                    temporary = Path(stream.name)
                    stream.write(rendered)
                os.replace(temporary, args.output)
            finally:
                if temporary is not None:
                    temporary.unlink(missing_ok=True)
        verb = "Checked" if args.check else "Wrote"
        print(f"{verb} {result['inventory_count']} GH2 entries, {len(result['category_counts'])} categories; all compatibility statuses UNKNOWN")
        print(f"Source export: {result['source']['rhino_gh2_version']} / {result['source']['exported_at_utc']}")
        return 0
    except (OSError, ValueError, UnicodeError) as error:
        print(f"GH2 inventory update failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
