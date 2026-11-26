#!/usr/bin/env python3
"""Inventory Cargo manifest declarations and existing license evidence, offline.

Requires Python 3.11+. Does not download dependencies, execute Cargo, select a
license, or read metadata-provided manifest/license paths. See the companion
dependency metadata document for the evidence boundaries.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile
import tomllib
import unittest
from urllib.parse import urlsplit

MAX_INPUT = 64 * 1024 * 1024
MAX_TEXT = 2 * 1024 * 1024
MAX_PACKAGES = 10000
LEDGERS = (
    "licenses/windows-dependency-audit.json",
    "licenses/additional-dependency-audit.json",
)
HEX64 = re.compile(r"[0-9a-fA-F]{64}\Z")
NAME = re.compile(r"[A-Za-z0-9_-]+\Z")
TOKEN = re.compile(r"\s*(\(|\)|[A-Za-z0-9][A-Za-z0-9.+:-]*)")
GNU = re.compile(r"^(?:A?GPL|LGPL|GFDL)(?:-|$|v?\d)", re.IGNORECASE)


class AuditError(ValueError):
    """Invalid evidence; messages intentionally omit external paths/content."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AuditError(message)


def text_field(value: object, label: str, *, nullable: bool = False,
               limit: int = 4096) -> str | None:
    if value is None and nullable:
        return None
    require(isinstance(value, str) and 0 < len(value) <= limit,
            "invalid " + label)
    require(not any(ord(ch) < 32 for ch in value), "control character in " + label)
    try:
        value.encode("utf-8")
    except UnicodeError as error:
        raise AuditError("invalid Unicode in " + label) from error
    return value


def unique_object(pairs: list[tuple[str, object]]) -> dict:
    result = {}
    for key, value in pairs:
        require(key not in result, "duplicate JSON object key")
        result[key] = value
    return result


def parse_json(data: bytes) -> dict:
    require(len(data) <= MAX_INPUT, "JSON input exceeds size limit")
    try:
        value = json.loads(data, object_pairs_hook=unique_object,
                           parse_constant=lambda _: (_ for _ in ()).throw(
                               AuditError("non-finite JSON number")))
    except (UnicodeError, json.JSONDecodeError, RecursionError) as error:
        raise AuditError("malformed JSON input") from error
    require(isinstance(value, dict), "JSON root must be an object")
    return value


def source_evidence(raw: str | None) -> dict:
    if raw is None:
        return {"kind": "path", "value": None, "status": "path_not_disclosed"}
    text_field(raw, "package source")
    # Source IDs are opaque in Cargo. Retain only public URL-shaped IDs without
    # credentials, local file paths, or secret-looking query parameters.
    kind, separator, address = raw.partition("+")
    try:
        parsed = urlsplit(address)
        safe_query = all(
            item.partition("=")[0] in {"rev", "branch", "tag"}
            for item in parsed.query.split("&") if item
        )
        public = (separator and kind in {"registry", "sparse", "git"}
                  and parsed.scheme in {"http", "https"}
                  and bool(parsed.hostname) and parsed.username is None
                  and parsed.password is None and safe_query)
    except ValueError:
        public = False
    if public:
        return {"kind": kind, "value": raw, "status": "cargo_source_id"}
    return {"kind": "other", "value": None,
            "status": "unrecognized_or_private_source_redacted",
            "identifier_sha256": hashlib.sha256(raw.encode()).hexdigest()}


def license_file_evidence(value: object, manifest: object) -> dict:
    if value is None:
        return {"declared": False, "path": None, "status": "not_declared"}
    raw = text_field(value, "license_file").replace("\\", "/")
    base = text_field(manifest, "manifest_path").replace("\\", "/")
    # Lexical handling only: none of these external paths are opened.
    parent = base.rsplit("/", 1)[0] + "/" if "/" in base else ""
    absolute = raw.startswith("/") or bool(re.match(r"^[A-Za-z]:/", raw))
    relative = raw[len(parent):] if absolute and parent and raw.startswith(parent) else raw
    pieces = relative.split("/")
    safe = (not relative.startswith("/") and not re.match(r"^[A-Za-z]:", relative)
            and all(piece not in {"", ".", ".."} for piece in pieces))
    return {
        "declared": True,
        "path": relative if safe else None,
        "status": ("manifest_relative_path_not_opened" if safe
                   else "external_or_ambiguous_path_not_opened"),
    }


def license_evidence(value: object, file_declared: bool) -> dict:
    if value is None or value == "":
        return {"expression": value, "syntax": "missing",
                "license_tokens": [], "exception_tokens": [],
                "gnu_family_tokens": [], "mpl_tokens": [],
                "has_declared_alternative_without_gnu_tokens": None,
                "review_reasons": ["license_file_only" if file_declared
                                   else "missing_license_declaration"]}
    expression = text_field(value, "license expression")
    tokens = []
    position = 0
    while position < len(expression.rstrip()):
        match = TOKEN.match(expression, position)
        if not match:
            break
        tokens.append(match.group(1))
        position = match.end()
    licenses: set[str] = set()
    exceptions: set[str] = set()
    cursor = 0

    def atom(depth: int) -> bool:
        nonlocal cursor
        require(depth < 64 and cursor < len(tokens), "invalid SPDX grammar")
        token = tokens[cursor]
        cursor += 1
        if token == "(":
            result = either(depth + 1)
            require(cursor < len(tokens) and tokens[cursor] == ")",
                    "invalid SPDX parentheses")
            cursor += 1
            return result
        require(token not in {"AND", "OR", "WITH", ")"}, "invalid SPDX identifier")
        licenses.add(token)
        result = GNU.match(token) is None
        if cursor < len(tokens) and tokens[cursor] == "WITH":
            cursor += 1
            require(cursor < len(tokens) and tokens[cursor] not in
                    {"AND", "OR", "WITH", "(", ")"}, "invalid SPDX exception")
            exceptions.add(tokens[cursor])
            cursor += 1
        return result

    def both(depth: int) -> bool:
        nonlocal cursor
        result = atom(depth)
        while cursor < len(tokens) and tokens[cursor] == "AND":
            cursor += 1
            right = atom(depth)
            result = result and right
        return result

    def either(depth: int) -> bool:
        nonlocal cursor
        result = both(depth)
        while cursor < len(tokens) and tokens[cursor] == "OR":
            cursor += 1
            right = both(depth)
            result = result or right
        return result

    alternative = None
    valid = position == len(expression.rstrip()) and len(tokens) <= 1024
    if valid:
        try:
            alternative = either(0)
            valid = cursor == len(tokens)
        except AuditError:
            valid = False
    # Preserve recognizable family tokens even when syntax is ambiguous.
    scanned = re.findall(r"[A-Za-z0-9][A-Za-z0-9.+:-]*", expression)
    gnu = sorted({token for token in scanned if GNU.match(token)})
    mpl = sorted({token for token in scanned if token.upper().startswith("MPL-")})
    review = []
    if not valid:
        review.append("ambiguous_or_unsupported_expression_syntax")
        alternative = None
    if gnu:
        review.append("gnu_family_declaration_requires_review")
    if mpl:
        review.append("mpl_declaration_requires_review")
    if any("LicenseRef-" in token or "DocumentRef-" in token for token in scanned):
        review.append("custom_license_reference_requires_text_review")
    return {
        "expression": expression,
        "syntax": "spdx_expression_grammar_only" if valid else "ambiguous",
        "license_tokens": sorted(licenses) if valid else [],
        "exception_tokens": sorted(exceptions) if valid else [],
        "gnu_family_tokens": gnu,
        "mpl_tokens": mpl,
        "has_declared_alternative_without_gnu_tokens": alternative,
        "review_reasons": sorted(review),
    }


def lock_inventory(data: bytes) -> dict:
    require(len(data) <= MAX_INPUT, "Cargo.lock exceeds size limit")
    try:
        lock = tomllib.loads(data.decode("utf-8"))
    except (UnicodeError, tomllib.TOMLDecodeError) as error:
        raise AuditError("malformed Cargo.lock") from error
    packages = lock.get("package")
    require(isinstance(packages, list) and len(packages) <= MAX_PACKAGES,
            "Cargo.lock package list missing or excessive")
    result = {}
    for package in packages:
        require(isinstance(package, dict), "invalid Cargo.lock package")
        name = text_field(package.get("name"), "locked package name")
        version = text_field(package.get("version"), "locked package version")
        source = text_field(package.get("source"), "locked package source", nullable=True)
        key = (name, version, source)
        require(key not in result, "duplicate Cargo.lock package identity")
        checksum = package.get("checksum")
        require(checksum is None or
                (isinstance(checksum, str) and HEX64.fullmatch(checksum)),
                "invalid Cargo.lock checksum")
        result[key] = checksum.lower() if checksum else None
    return result


def tracked_paths(root: Path) -> set[str]:
    try:
        process = subprocess.run(
            ["git", "-C", str(root), "ls-tree", "-r", "-z", "HEAD", "--",
             "Cargo.lock", "licenses"],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=20,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise AuditError("cannot inspect tracked license evidence") from error
    require(process.returncode == 0 and len(process.stdout) <= MAX_TEXT,
            "cannot inspect tracked license evidence")
    result = set()
    for row in process.stdout.split(b"\0"):
        if not row:
            continue
        header, separator, path = row.partition(b"\t")
        require(bool(separator), "invalid git tree record")
        fields = header.split()
        require(len(fields) == 3, "invalid git tree record")
        if fields[0] in {b"100644", b"100755"} and fields[1] == b"blob":
            try:
                result.add(path.decode("utf-8"))
            except UnicodeError as error:
                raise AuditError("non-UTF-8 tracked evidence path") from error
    return result


def repository_bytes(root: Path, relative: str, tracked: set[str]) -> bytes:
    require(isinstance(relative, str) and "\\" not in relative
            and not relative.startswith("/") and ":" not in relative,
            "unsafe repository evidence path")
    parts = relative.split("/")
    require(all(part not in {"", ".", ".."} for part in parts),
            "unsafe repository evidence path")
    require(relative == "Cargo.lock" or relative.startswith("licenses/"),
            "repository evidence path outside allowlist")
    require(relative in tracked, "evidence is not a tracked regular file")
    current = root
    for index, part in enumerate(parts):
        current /= part
        try:
            info = current.lstat()
        except OSError as error:
            raise AuditError("tracked evidence is missing or unreadable") from error
        require(not stat.S_ISLNK(info.st_mode), "symlink evidence rejected")
        require(stat.S_ISDIR(info.st_mode) if index + 1 < len(parts)
                else stat.S_ISREG(info.st_mode), "non-regular evidence rejected")
    require(info.st_size <= MAX_TEXT, "repository evidence exceeds size limit")
    try:
        with current.open("rb") as handle:
            data = handle.read(MAX_TEXT + 1)
    except OSError as error:
        raise AuditError("tracked evidence is unreadable") from error
    require(len(data) <= MAX_TEXT, "repository evidence exceeds size limit")
    return data


def checked_in_evidence(root: Path, tracked: set[str], locks: dict) -> tuple[dict, list]:
    evidence: dict[tuple, list] = {}
    statuses = []
    for ledger_path in LEDGERS:
        if ledger_path not in tracked:
            statuses.append({"path": ledger_path, "status": "not_tracked"})
            continue
        ledger = parse_json(repository_bytes(root, ledger_path, tracked))
        packages = ledger.get("packages")
        require(isinstance(packages, list) and len(packages) <= MAX_PACKAGES,
                "invalid checked-in license ledger")
        statuses.append({"path": ledger_path, "status": "read",
                         "packages": len(packages)})
        for package in packages:
            require(isinstance(package, dict), "invalid license ledger package")
            name = text_field(package.get("name"), "license ledger package name")
            version = text_field(package.get("version"), "license ledger package version")
            checksum = package.get("registry_checksum")
            matching = [key for key, actual in locks.items()
                        if key[:2] == (name, version) and actual is not None
                        and isinstance(checksum, str) and actual == checksum.lower()
                        and key[2] is not None and key[2].startswith(("registry+", "sparse+"))]
            files = package.get("license_files", [])
            require(isinstance(files, list) and len(files) <= 128,
                    "invalid checked-in license file list")
            if len(matching) != 1:
                statuses.append({"path": ledger_path, "package": name + "@" + version,
                                 "status": "package_checksum_not_uniquely_attributable"})
                continue
            for record in files:
                require(isinstance(record, dict), "invalid license file record")
                relative = record.get("path")
                expected = record.get("sha256")
                require(isinstance(relative, str) and isinstance(expected, str)
                        and HEX64.fullmatch(expected), "invalid license text evidence")
                # Reject malformed paths instead of copying them into a report.
                require(relative.startswith("licenses/") and "\\" not in relative
                        and ":" not in relative
                        and all(part not in {"", ".", ".."} for part in relative.split("/")),
                        "unsafe license ledger path")
                result = {"path": relative, "ledger": ledger_path,
                          "expected_sha256": expected.lower(), "actual_sha256": None}
                try:
                    data = repository_bytes(root, relative, tracked)
                    actual = hashlib.sha256(data).hexdigest()
                    result.update(actual_sha256=actual,
                                  status=("matches_recorded_text_hash"
                                          if actual == expected.lower() else "text_hash_mismatch"))
                except AuditError as error:
                    result["status"] = "unverified"
                    result["reason"] = str(error)
                evidence.setdefault(matching[0], []).append(result)
    for records in evidence.values():
        records.sort(key=lambda row: (row["path"], row["ledger"]))
    return evidence, statuses


def build_inventory(metadata: dict, lock_data: bytes,
                    checked: dict | None = None, ledgers: list | None = None) -> dict:
    require(type(metadata.get("version")) is int and metadata["version"] == 1,
            "cargo metadata --format-version 1 is required")
    packages = metadata.get("packages")
    require(isinstance(packages, list) and 0 < len(packages) <= MAX_PACKAGES,
            "metadata packages missing or excessive")
    members = metadata.get("workspace_members")
    require(isinstance(members, list) and all(isinstance(item, str) for item in members)
            and len(set(members)) == len(members), "invalid workspace member IDs")
    resolve = metadata.get("resolve")
    require(isinstance(resolve, dict) and isinstance(resolve.get("nodes"), list),
            "dependency resolution missing; do not use --no-deps")
    locks = lock_inventory(lock_data)
    checked = checked or {}
    rows = []
    ids = set()
    identities = set()
    by_id = {}
    seen_locked = set()
    for package in packages:
        require(isinstance(package, dict), "invalid metadata package")
        raw_id = text_field(package.get("id"), "package ID")
        require(raw_id not in ids, "duplicate metadata package ID")
        ids.add(raw_id)
        name = text_field(package.get("name"), "package name")
        version = text_field(package.get("version"), "package version")
        require(NAME.fullmatch(name) is not None
                and re.fullmatch(r"[0-9A-Za-z.+-]+", version) is not None,
                "invalid package name or version")
        raw_source = text_field(package.get("source"), "package source", nullable=True)
        identity = (name, version, raw_source)
        require(identity not in identities, "duplicate metadata package identity")
        identities.add(identity)
        require(identity in locks, "metadata package does not match Cargo.lock")
        seen_locked.add(identity)
        checksum = locks[identity]
        optional_checksum = package.get("checksum")
        require(optional_checksum is None or
                (isinstance(optional_checksum, str) and HEX64.fullmatch(optional_checksum)
                 and optional_checksum.lower() == checksum),
                "metadata checksum disagrees with Cargo.lock")
        file_evidence = license_file_evidence(package.get("license_file"),
                                              package.get("manifest_path"))
        licensing = license_evidence(package.get("license"), file_evidence["declared"])
        sources = source_evidence(raw_source)
        review = list(licensing["review_reasons"])
        if sources["status"] == "unrecognized_or_private_source_redacted":
            review.append("source_identifier_requires_review")
        notices = checked.get(identity, [])
        if not notices:
            review.append("no_checksum_attributed_checked_in_license_text")
        if any(item["status"] != "matches_recorded_text_hash" for item in notices):
            review.append("checked_in_license_text_requires_review")
        row = {
            "name": name, "version": version, "source": sources,
            "workspace_member": raw_id in members,
            "checksum": checksum,
            "checksum_evidence": ("Cargo.lock_declaration_archive_not_rehashed"
                                  if checksum else "no_registry_checksum_in_Cargo.lock"),
            "license": licensing, "license_file": file_evidence,
            "checked_in_license_texts": notices,
            "review_reasons": sorted(set(review)),
        }
        rows.append(row)
        by_id[raw_id] = row
    require(set(members) <= ids, "workspace member absent from packages")
    nodes_seen = set()
    for node in resolve["nodes"]:
        require(isinstance(node, dict), "invalid resolution node")
        raw_id = node.get("id")
        require(isinstance(raw_id, str) and raw_id in ids and raw_id not in nodes_seen,
                "missing or duplicate resolution package")
        nodes_seen.add(raw_id)
        features = node.get("features", [])
        require(isinstance(features, list)
                and all(isinstance(item, str) and len(item) <= 512 for item in features),
                "invalid resolved feature list")
        by_id[raw_id]["resolved_features"] = sorted(set(features))
    require(nodes_seen == ids, "metadata packages and resolution nodes differ")
    rows.sort(key=lambda row: (row["name"], row["version"],
                              json.dumps(row["source"], sort_keys=True)))
    missing = [
        {"name": key[0], "version": key[1], "source": source_evidence(key[2]),
         "checksum": locks[key]}
        for key in sorted(locks, key=lambda key: (key[0], key[1], key[2] or ""))
        if key not in seen_locked
    ]
    return {
        "schema_version": 1,
        "generator": "scripts/audit_dependencies.py",
        "metadata_format_version": 1,
        "scope": "Cargo metadata resolved packages for the supplied invocation; not a linked-binary audit",
        "evidence_boundaries": {
            "license_declarations_are_legal_approval": False,
            "spdx_identifier_catalog_validated": False,
            "registry_archives_rehashed": False,
            "external_manifest_or_license_paths_opened": False,
            "license_text_completeness_claimed": False,
            "runtime_and_font_notices_included": False,
            "checked_in_attribution": "exact name/version/registry checksum from fixed historical ledgers",
        },
        "cargo_lock_sha256": hashlib.sha256(lock_data).hexdigest(),
        "counts": {
            "packages": len(rows),
            "workspace_packages": sum(row["workspace_member"] for row in rows),
            "license_declarations": sum(row["license"]["syntax"] != "missing" for row in rows),
            "missing_license_declarations": sum(row["license"]["syntax"] == "missing" for row in rows),
            "ambiguous_license_expressions": sum(row["license"]["syntax"] == "ambiguous" for row in rows),
            "gnu_family_declarations": sum(bool(row["license"]["gnu_family_tokens"]) for row in rows),
            "mpl_declarations": sum(bool(row["license"]["mpl_tokens"]) for row in rows),
            "packages_with_checked_in_text": sum(bool(row["checked_in_license_texts"]) for row in rows),
            "checked_in_text_hash_matches": sum(
                item["status"] == "matches_recorded_text_hash"
                for row in rows for item in row["checked_in_license_texts"]),
            "packages_with_review_reasons": sum(bool(row["review_reasons"]) for row in rows),
            "lock_packages_absent_from_metadata": len(missing),
        },
        "ledger_checks": ledgers or [],
        "lock_packages_absent_from_metadata": missing,
        "packages": rows,
    }


def serialize(report: dict) -> bytes:
    return (json.dumps(report, ensure_ascii=True, sort_keys=True, indent=2,
                       allow_nan=False) + "\n").encode("ascii")


def atomic_write(path: Path, content: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    require(not path.is_symlink(), "output symlink rejected")
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".dependency-audit-",
                                         delete=False) as handle:
            temporary = Path(handle.name)
            handle.write(content)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
        temporary = None
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


class SelfTests(unittest.TestCase):
    @staticmethod
    def fixture() -> tuple[dict, bytes]:
        source = "registry+https://github.com/rust-lang/crates.io-index"
        metadata = {
            "version": 1, "workspace_members": ["path:/runner/private/root"],
            "packages": [
                {"id": "path:/runner/private/root", "name": "app", "version": "0.1.0",
                 "source": None, "license": "MIT OR Apache-2.0", "license_file": None,
                 "manifest_path": "/runner/private/root/Cargo.toml"},
                {"id": "registry:dep@1.0.0", "name": "dep", "version": "1.0.0",
                 "source": source, "license": None, "license_file": None,
                 "manifest_path": "/runner/private/cache/dep/Cargo.toml"},
            ],
            "resolve": {"nodes": [
                {"id": "path:/runner/private/root", "features": ["z", "a"]},
                {"id": "registry:dep@1.0.0", "features": []},
            ]},
        }
        lock = ('version = 4\n[[package]]\nname = "app"\nversion = "0.1.0"\n'
                '[[package]]\nname = "dep"\nversion = "1.0.0"\nsource = "'
                + source + '"\nchecksum = "' + "a" * 64 + '"\n').encode()
        return metadata, lock

    def test_malformed_input(self) -> None:
        for value in [b"{", b"[]", b'{"version":1,"version":1}', b'{"x":NaN}']:
            with self.assertRaises(AuditError):
                parse_json(value)
        metadata, lock = self.fixture()
        metadata["resolve"] = None
        with self.assertRaises(AuditError):
            build_inventory(metadata, lock)

    def test_missing_license_and_mpl_preserved(self) -> None:
        metadata, lock = self.fixture()
        report = build_inventory(metadata, lock)
        self.assertEqual(report["counts"]["missing_license_declarations"], 1)
        self.assertIn("missing_license_declaration", report["packages"][1]["review_reasons"])
        metadata["packages"][1]["license"] = "MPL-2.0+"
        report = build_inventory(metadata, lock)
        self.assertEqual(report["packages"][1]["license"]["expression"], "MPL-2.0+")
        self.assertEqual(report["counts"]["mpl_declarations"], 1)

    def test_duplicate_id_and_identity(self) -> None:
        for change_id in [False, True]:
            metadata, lock = self.fixture()
            duplicate = dict(metadata["packages"][1])
            if change_id:
                duplicate["id"] = "different-opaque-id"
            metadata["packages"].append(duplicate)
            with self.assertRaises(AuditError):
                build_inventory(metadata, lock)

    def test_deterministic_order_and_private_paths(self) -> None:
        metadata, lock = self.fixture()
        first = serialize(build_inventory(metadata, lock))
        metadata["packages"].reverse()
        metadata["resolve"]["nodes"].reverse()
        metadata["resolve"]["nodes"][1]["features"].reverse()
        self.assertEqual(first, serialize(build_inventory(metadata, lock)))
        self.assertNotIn(b"/runner", first)

    def test_spdx_or_is_not_blanket_rejection(self) -> None:
        self.assertTrue(license_evidence("MIT OR GPL-3.0-only", False)[
            "has_declared_alternative_without_gnu_tokens"])
        self.assertFalse(license_evidence("MIT AND GPL-3.0-only", False)[
            "has_declared_alternative_without_gnu_tokens"])
        self.assertTrue(license_evidence(
            "(MIT OR GPL-2.0-only WITH Classpath-exception-2.0) AND Apache-2.0",
            False)["has_declared_alternative_without_gnu_tokens"])
        self.assertEqual(license_evidence("MIT/Apache-2.0", False)["syntax"], "ambiguous")
        self.assertIsNone(license_evidence("MIT OR", False)[
            "has_declared_alternative_without_gnu_tokens"])

    def test_no_external_license_paths_or_private_source(self) -> None:
        evidence = license_file_evidence(
            "/private/cache/dep/LICENSE", "/private/cache/dep/Cargo.toml")
        self.assertEqual(evidence["path"], "LICENSE")
        for value in ["../secret", "/etc/passwd", "C:\\secret\\key", "a/../key"]:
            self.assertIsNone(license_file_evidence(
                value, "/private/cache/dep/Cargo.toml")["path"])
        for source in ["git+file:///private/repo", "git+https://token@example.com/repo",
                       "registry+https://example.com/repo?token=secret"]:
            self.assertIsNone(source_evidence(source)["value"])

    def test_checked_text_integrity_and_traversal_rejected(self) -> None:
        metadata, lock = self.fixture()
        locks = lock_inventory(lock)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "licenses/crates/dep-1.0.0/LICENSE"
            path.parent.mkdir(parents=True)
            path.write_bytes(b"known license text\n")
            ledger = {
                "packages": [{"name": "dep", "version": "1.0.0",
                              "registry_checksum": "a" * 64,
                              "license_files": [{"path": "licenses/crates/dep-1.0.0/LICENSE",
                                                "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}]}]
            }
            ledger_path = root / LEDGERS[0]
            ledger_path.write_bytes(serialize(ledger))
            tracked = {LEDGERS[0], "licenses/crates/dep-1.0.0/LICENSE"}
            checked, statuses = checked_in_evidence(root, tracked, locks)
            report = build_inventory(metadata, lock, checked, statuses)
            self.assertEqual(report["counts"]["checked_in_text_hash_matches"], 1)
            path.write_bytes(b"changed\n")
            checked, _ = checked_in_evidence(root, tracked, locks)
            self.assertEqual(next(iter(checked.values()))[0]["status"], "text_hash_mismatch")
            for relative in ["licenses/../../secret", "/etc/passwd", "licenses\\secret"]:
                with self.assertRaises(AuditError):
                    repository_bytes(root, relative, tracked)
            path.unlink()
            try:
                path.symlink_to(ledger_path)
            except (OSError, NotImplementedError):
                return
            with self.assertRaises(AuditError):
                repository_bytes(root, "licenses/crates/dep-1.0.0/LICENSE", tracked)

    def test_lock_mismatch_and_resolution_mismatch(self) -> None:
        metadata, lock = self.fixture()
        metadata["packages"][1]["version"] = "2.0.0"
        with self.assertRaises(AuditError):
            build_inventory(metadata, lock)
        metadata, lock = self.fixture()
        metadata["resolve"]["nodes"].pop()
        with self.assertRaises(AuditError):
            build_inventory(metadata, lock)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--metadata", type=Path, help="cargo metadata JSON file")
    parser.add_argument("--output", type=Path,
                        default=Path("docs/development/dependency-manifests.json"))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        # unittest reports only test status, never fixtures or dependency contents.
        result = unittest.TextTestRunner(verbosity=1).run(
            unittest.defaultTestLoader.loadTestsFromTestCase(SelfTests))
        return 0 if result.wasSuccessful() else 1
    if args.metadata is None:
        parser.error("--metadata is required unless --self-test is used")
    root = Path(__file__).resolve().parents[1]
    try:
        require(not args.metadata.is_symlink()
                and args.metadata.is_file(), "metadata must be a regular file")
        require(args.metadata.stat().st_size <= MAX_INPUT, "metadata exceeds size limit")
        require(args.output.resolve() != args.metadata.resolve(),
                "output must not replace metadata input")
        metadata = parse_json(args.metadata.read_bytes())
        tracked = tracked_paths(root)
        lock_data = repository_bytes(root, "Cargo.lock", tracked)
        locks = lock_inventory(lock_data)
        checked, ledgers = checked_in_evidence(root, tracked, locks)
        report = build_inventory(metadata, lock_data, checked, ledgers)
        atomic_write(args.output, serialize(report))
    except (AuditError, OSError) as error:
        # OSError text may contain absolute paths; keep diagnostics bounded.
        message = str(error) if isinstance(error, AuditError) else "filesystem operation failed"
        print("dependency audit failed: " + message, file=sys.stderr)
        return 1
    counts = report["counts"]
    print("Dependency manifest inventory: "
          f"{counts['packages']} packages; "
          f"{counts['missing_license_declarations']} missing declarations; "
          f"{counts['ambiguous_license_expressions']} ambiguous expressions; "
          f"{counts['gnu_family_declarations']} GNU-family declarations for review; "
          f"{counts['mpl_declarations']} MPL declarations for review; "
          f"{counts['checked_in_text_hash_matches']} checked-in text hashes match.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
